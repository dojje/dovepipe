use std::{error, time::Duration};

#[cfg(feature = "logging")]
use log::debug;
#[cfg(feature = "logging")]
use log::info;
use tokio::{fs::File, net::ToSocketAddrs, time};

use crate::{get_buf, punch_hole, read_position, send_unil_recv, u8s_to_u64, Source};

async fn get_file_buf_from_msg_num<Buf>(
    msg: u64,
    file: &File,
    buf_size: u64,
    buf: Buf,
) -> Result<(Buf, usize), Box<dyn error::Error + Send + Sync>>
where
    Buf: AsMut<[u8]> + Send + 'static,
{
    read_position(&file, buf, msg * buf_size).await
}

/// # This is used to send files
///
/// ## Example
/// ```
/// let port = 3456;
/// println!("my ip: 127.0.0.1:{}", port);
///
/// send_file(
///     Source::Port(port),
///     "./examples/file_to_send.txt",
///     "127.0.0.1:7890",
/// )
/// .await
/// .expect("error when sending file");
/// ```
///
/// This takes in a source which is the udp socket to send from
///
/// This will listen for any recievers on port 3456 on ip 127.0.0.1. *Note: localhost and 127.0.0.1 are the same.*
///
pub async fn send_file<T: Clone + 'static + ToSocketAddrs + Send + Copy + std::fmt::Display>(
    source: Source,
    file_name: &str,
    reciever: T,
) -> Result<(), Box<dyn error::Error + Send + Sync>> {
    #[cfg(feature = "logging")]
    debug!("reciever ip: {}", reciever);

    let sock = source.into_socket().await;

    // TODO: Send amount of bytes in file
    // TODO: Add function for sending until request stops
    // TODO: Make different ways to keep track of progress
    let sock_ = sock.clone();
    let reciever_ = reciever.clone();
    let holepuncher = tokio::task::spawn(async move {
        let sock = sock_;

        let mut holepunch_interval = time::interval(Duration::from_secs(5));
        loop {
            punch_hole(&sock, reciever_).await.unwrap();

            holepunch_interval.tick().await;
        }
    });

    time::sleep(Duration::from_millis(1000)).await;

    let input_file = File::open(file_name).await?;
    let file_len = input_file.metadata().await?.len();

    let file_len_arr = file_len.to_be_bytes();
    let msg = [
        8,
        file_len_arr[0],
        file_len_arr[1],
        file_len_arr[2],
        file_len_arr[3],
        file_len_arr[4],
        file_len_arr[5],
        file_len_arr[6],
        file_len_arr[7],
    ];

    #[cfg(feature = "logging")]
    debug!("sending file size...");
    let mut has_sent = false;
    loop {
        let mut buf = [0u8; 508];
        let sleep = time::sleep(Duration::from_millis(1500));
        tokio::select! {
            _ = sleep => {
                if has_sent {
                    break;
                }
            }

            amt = sock.recv(&mut buf) => {
                let amt = amt?;
                let buf = &buf[0..amt];

                #[cfg(feature = "logging")]
                debug!("got msg: {:?}", buf);

                if buf.len() == 1 && buf[0] == 9 {
                    sock.send_to(&msg, reciever).await?;
                    has_sent = true;
                }
            }

        }
    }
    #[cfg(feature = "logging")]
    debug!("has sent file size");

    // Udp messages should be 508 bytes
    // 8 of those bytes are used for checking order of recieving bytes
    // The rest 500 bytes are used to send the file
    // The file gets send 500 bytes
    let mut offset = 0;
    let mut msg_num: u64 = 0;

    loop {
        let (file_buf, amt) = read_position(&input_file, [0u8; 500], offset).await?;

        let buf = get_buf(&msg_num, &file_buf[0..amt]);

        sock.send_to(&buf, reciever).await?;

        offset += 500;
        if offset >= file_len {
            break;
        }
        #[cfg(feature = "logging")]
        info!("Progress: {}%", offset * 100 / file_len);

        msg_num += 1;
    }

    loop {
        #[cfg(feature = "logging")]
        info!("Sending done, getting dropped messages");

        // sending done
        let mut buf = [0u8; 508];
        let amt = send_unil_recv(&sock, &[5], &reciever, &mut buf, 100).await?;

        // This will be an array of u64s with missed things
        // The first will be a message
        let buf = &buf[0..amt];
        if buf[0] == 7 {
            // 6 is missed
            holepuncher.abort();
            #[cfg(feature = "logging")]
            info!("No dropped messages left, finishing...");
            return Ok(());
        } else if buf[0] != 6 {
            continue;
        }

        // Some messages were missed

        let missed = &buf[1..];

        #[cfg(feature = "logging")]
        info!("Dropped messages: {}", missed.len() / 8);
        for i in 0..(missed.len() / 8) {
            let j = i * 8;
            // Convert bytes to offset
            let missed_msg = u8s_to_u64(&missed[j..j + 8])?;

            // Read from file
            let (file_buf, amt) =
                get_file_buf_from_msg_num(missed_msg, &input_file, 500, [0u8; 500]).await?;
            let file_buf = &file_buf[0..amt];
            let buf = get_buf(&missed_msg, file_buf);

            sock.send_to(&buf, reciever).await?;
        }
    }
}

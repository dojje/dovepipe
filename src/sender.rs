use std::{error, fs::File, net::SocketAddr, thread, time::Duration, sync::Arc};

#[cfg(feature = "logging")]
use log::{debug};
use tokio::{net::UdpSocket, time};


use crate::{get_buf, punch_hole, read_position, send_unil_recv, u8s_to_u64};

fn get_file_buf_from_msg_num(
    msg: u64,
    file: &File,
    buf_size: u64,
    buf: &mut [u8],
) -> Result<usize, Box<dyn error::Error>> {
    let amt = read_position(&file, buf, msg * buf_size)?;

    Ok(amt)
}

// Intervals
const SEND_FILE_INTERVAL: u64 = 1500;

pub async fn send_file(
    sock: Arc<UdpSocket>,
    file_name: &str,
    reciever: SocketAddr,
) -> Result<(), Box<dyn error::Error>> {
    #[cfg(feature = "logging")]
    debug!("reciever ip: {}", reciever);

    // TODO: Send amount of bytes in file
    // TODO: Add function for sending until request stops
    // TODO: Make different ways to keep track of progress
    let sock_ = sock.clone();
    let holepuncher = tokio::task::spawn(async move {
        let sock = sock_;

        let mut holepunch_interval = time::interval(Duration::from_secs(5));
        loop {
            punch_hole(&sock, reciever).await.unwrap();

            holepunch_interval.tick().await;
        }
    });

    thread::sleep(Duration::from_millis(1000));

    let input_file = File::open(file_name)?;
    let file_len = input_file.metadata()?.len();

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

    let mut send_interval = time::interval(Duration::from_micros(SEND_FILE_INTERVAL));
    loop {
        let mut file_buf = [0u8; 500];
        let amt = read_position(&input_file, &mut file_buf, offset)?;

        let buf = get_buf(&msg_num, &file_buf[0..amt]);

        send_interval.tick().await;

        sock.send_to(&buf, reciever).await?;

        offset += 500;
        if offset >= file_len {
            break;
        }

        msg_num += 1;
    }

    #[cfg(feature = "logging")]
    debug!("first pass of sending done");

    loop {
        let mut buf = [0u8; 508];
        let amt = send_unil_recv(&sock, &[5], &reciever, &mut buf, 100).await?;

        // This will be an array of u64s with missed things
        // The first will be a message
        let buf = &buf[0..amt];
        if buf[0] == 7 {
            // 6 is missed
            holepuncher.abort();
            return Ok(());
        }

        let missed = &buf[1..];
        for i in 0..(missed.len() / 8) {
            let j = i * 8;
            // Convert bytes to offset
            let missed_msg = u8s_to_u64(&missed[j..j + 8])?;
            let mut file_buf = [0u8; 500];
            // Read from file
            let amt = get_file_buf_from_msg_num(missed_msg, &input_file, 500, &mut file_buf)?;
            let file_buf = &file_buf[0..amt];
            let buf = get_buf(&missed_msg, file_buf);

            send_interval.tick().await;
            sock.send_to(&buf, reciever).await?;
        }
    }

}

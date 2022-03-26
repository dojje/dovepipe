use async_trait::async_trait;
#[cfg(feature = "logging")]
use log::debug;
#[cfg(feature = "logging")]
use log::info;
use tokio::sync::RwLock;
use tokio::task;
use std::sync::Arc;
#[cfg(feature = "logging")]
use std::time::SystemTime;
#[cfg(feature = "logging")]
use std::time::UNIX_EPOCH;
use std::{
    error,
    io::{self},
    time::Duration,
};
use tokio::{
    fs::{remove_file, File, OpenOptions},
    net::ToSocketAddrs,
    time,
};

use crate::{read_position, recv, send_until_recv, u8s_to_u64, write_position, Source};

#[async_trait]
trait ProgressTracker {
    async fn recv_msg(&mut self, msg_num: u64) -> Result<(), Box<dyn error::Error + Send + Sync>>;
    async fn get_unrecv(&self) -> Result<Vec<u64>, Box<dyn error::Error + Send + Sync>>;
    async fn destruct(&self);
}

struct FileProgTrack {
    filename: String,
    file: File,
    size: u64,
}

impl FileProgTrack {
    async fn new(filename: String, size: u64) -> Result<Self, Box<dyn error::Error>> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&filename)
            .await?;

        // Populate file with 0:s
        file.set_len(get_msg_amt(size)).await?;

        Ok(Self {
            filename,
            file,
            size,
        })
    }
}

#[async_trait]
impl ProgressTracker for FileProgTrack {
    async fn recv_msg(&mut self, msg_num: u64) -> Result<(), Box<dyn error::Error + Send + Sync>> {
        // Get position in index file
        let (offset, pos_in_offset) = get_pos_of_num(msg_num);

        // Read offset position from index file
        let (offset_buf, _) = read_position(&self.file, [0u8; 1], offset).await?;

        // Change the offset
        let mut offset_binary = to_binary(offset_buf[0]);
        offset_binary[pos_in_offset as usize] = true;
        let offset_buf = from_binary(offset_binary);

        // Write the offset
        write_position(&self.file, [offset_buf], offset).await?;

        Ok(())
    }

    async fn get_unrecv(&self) -> Result<Vec<u64>, Box<dyn error::Error + Send + Sync>> {
        let mut dropped: Vec<u64> = Vec::new();

        // let mut byte = num / 8;
        // let mut pos = num % 8;

        let total = if self.size % 500 == 0 {
            self.size / 500
        } else {
            self.size / 500 + 1
        };

        for byte in 0..self.file.metadata().await?.len() {
            // For every byte
            let ([bin], _) = read_position(&self.file, [0u8], byte).await?;
            let bin = to_binary(bin);

            let mut bit_pos = 0;
            for bit in bin {
                let num = get_num_of_pos(byte, bit_pos);
                // num starts it's counting from 0
                if num == total {
                    // Return if it has checked every bit
                    return Ok(dropped);
                }
                if !bit {
                    dropped.push(num);
                    if dropped.len() == 63 {
                        return Ok(dropped);
                    }
                }

                bit_pos += 1;
            }
        }

        Ok(dropped)
    }

    async fn destruct(&self) {
        remove_file(&self.filename).await.unwrap()
    }
}

fn get_msg_amt(file_len: u64) -> u64 {
    if file_len % 500 == 0 {
        file_len / 500
    } else {
        file_len / 500 + 1
    }
}
struct MemProgTracker {
    tracker: Vec<u8>,
}

impl MemProgTracker {
    fn new(size: u64) -> Self {
        let tracker_size = get_msg_amt(size) as usize;
        let tracker = vec![0u8; tracker_size];

        Self { tracker }
    }
}

#[async_trait]
impl ProgressTracker for MemProgTracker {
    async fn recv_msg(&mut self, msg_num: u64) -> Result<(), Box<dyn error::Error + Send + Sync>> {
        // Get position in index file
        let (offset, pos_in_offset) = get_pos_of_num(msg_num);

        // Read offset position from index file

        // Change the offset
        let mut offset_binary = to_binary(self.tracker[offset as usize]);
        offset_binary[pos_in_offset as usize] = true;
        let offset_buf = from_binary(offset_binary);

        // Write the offset
        self.tracker[offset as usize] = offset_buf;

        Ok(())
    }

    async fn get_unrecv(&self) -> Result<Vec<u64>, Box<dyn error::Error + Send + Sync>> {
        let mut dropped: Vec<u64> = Vec::new();

        // let mut byte = num / 8;
        // let mut pos = num % 8;

        let total = get_msg_amt(self.tracker.len() as u64);

        let mut i = 0;
        for byte in &self.tracker {
            // For every byte
            let bin = to_binary(*byte);

            let mut bit_pos = 0;
            for bit in bin {
                let num = get_num_of_pos(i, bit_pos);
                // num starts it's counting from 0
                if num == total {
                    // Return if it has checked every bit
                    return Ok(dropped);
                }
                if !bit {
                    dropped.push(num);
                    if dropped.len() == 63 {
                        return Ok(dropped);
                    }
                }

                bit_pos += 1;
            }

            i += 1;
        }

        Ok(dropped)
    }

    async fn destruct(&self) {}
}

pub enum ProgressTracking {
    File(String),
    Memory,
}

fn get_offset(msg_num: u64) -> u64 {
    msg_num * 500
}

fn get_pos_of_num(num: u64) -> (u64, u8) {
    let cell = num / 8;
    let cellpos = num % 8;

    (cell, cellpos as u8)
}

fn get_num_of_pos(byte: u64, pos: u8) -> u64 {
    byte * 8 + pos as u64
}

fn to_binary(mut num: u8) -> [bool; 8] {
    let mut arr = [false; 8];

    if num >= 128 {
        arr[0] = true;
        num -= 128;
    }
    if num >= 64 {
        arr[1] = true;
        num -= 64;
    }
    if num >= 32 {
        arr[2] = true;
        num -= 32;
    }
    if num >= 16 {
        arr[3] = true;
        num -= 16;
    }
    if num >= 8 {
        arr[4] = true;
        num -= 8;
    }
    if num >= 4 {
        arr[5] = true;
        num -= 4;
    }
    if num >= 2 {
        arr[6] = true;
        num -= 2;
    }
    if num >= 1 {
        arr[7] = true;
        // num -= 1;
    }

    arr
}

fn from_binary(bin: [bool; 8]) -> u8 {
    let mut num = 0;
    if bin[0] {
        num += 128;
    }
    if bin[1] {
        num += 64;
    }
    if bin[2] {
        num += 32;
    }
    if bin[3] {
        num += 16;
    }
    if bin[4] {
        num += 8;
    }
    if bin[5] {
        num += 4;
    }
    if bin[6] {
        num += 2;
    }
    if bin[7] {
        num += 1;
    }

    num
}

async fn write_msg(
    buf: &[u8],
    out_file: &File,
) -> Result<u64, Box<dyn error::Error + Send + Sync>> {
    // Get msg num
    let msg_num = u8s_to_u64(&buf[0..8])?;

    let msg_offset = get_offset(msg_num);

    // Write the data of the msg to out_file
    let rest = buf[8..].to_owned();
    write_position(out_file, rest, msg_offset).await.unwrap();

    Ok(msg_num)
}

/// # This is used to recieve files
///
/// ## Sending example
///
/// This is taken from the official examples
/// ```
/// let port = 7890;
/// println!("my ip: 127.0.0.1:{}", port);
///
/// recv_file(
///     Source::Port(port),
///     &mut File::create("output_from_recv.txt").expect("could not create output file"),
///     "127.0.0.1:3456",
///     ProgressTracking::Memory,
/// )
/// .await
/// .expect("error when sending file");
/// ```
/// This takes in a source which is the UdpSocket to send recieve from.
///
/// This looks for any senders on port 7890 on ip 127.0.0.1.
/// *Note: 127.0.0.1 is the same as localhost*
///
/// When it finds one it will send the file
///
pub async fn recv_file<T>(
    source: Source,
    file: File,
    sender: T,
    progress_tracking: ProgressTracking,
) -> Result<(), Box<dyn error::Error + Send + Sync>>
where
    T: 'static + Clone + ToSocketAddrs + std::marker::Send + Copy, // This many traits is probalbly unnececery but it works
{
    let sock = source.into_socket().await;

    let sock_ = sock.clone();
    let sender_ = sender.clone();
    let holepuncher = tokio::task::spawn(async move {
        let sock = sock_;
        let sender = sender_;

        let mut holepunch_interval = time::interval(Duration::from_secs(5));
        loop {
            sock.send_to(&[255u8], sender).await.unwrap();

            holepunch_interval.tick().await;
        }
    });

    #[cfg(feature = "logging")]
    debug!("getting file size");

    // Recieve file size from sender
    let buf: [u8; 508];
    let amt = loop {
        let mut new_buf = [0u8; 508];

        // Send message to sender until a messge gets recieved
        let amt = send_until_recv(&*sock, &[9], &sender, &mut new_buf, 50).await?;

        #[cfg(feature = "logging")]
        debug!("got size msg: {:?}", &new_buf[0..amt]);

        if amt == 9 && new_buf[0] == 8 {
            buf = new_buf;
            break amt;
        }
    };
    let buf = &buf[0..amt];

    let size_be_bytes = &buf[1..];
    let size = u8s_to_u64(size_be_bytes)?;

    #[cfg(feature = "logging")]
    debug!("size: {}", size);
    // When the giver think it's done it should say that to the taker
    // the taker should check that it has recieved all packets
    // If not, the taker should send what messages are unsent
    // If there are too many for one message the other ones should be sent in the iteration

    // Create index file
    // TODO Check so that file doesn't already exist
    let mut prog_tracker: Box<dyn ProgressTracker> = match progress_tracking {
        ProgressTracking::File(filename) => {
            Box::new(FileProgTrack::new(filename, size).await.unwrap())
        }
        ProgressTracking::Memory => Box::new(MemProgTracker::new(size)),
    };

    // Create writer
    let msg_buffer: Arc<RwLock<Vec<Vec<u8>>>> = Arc::new(RwLock::new(Vec::new()));
    // Spawn writer task
    let msg_buffer_ = msg_buffer.clone();
    let msg_writer = task::spawn(async move {
        let msg_buffer = msg_buffer_;
        loop {
            time::sleep(Duration::from_millis(200)).await;

            // Write whole buffer to file
            let mut buf = msg_buffer.write().await;

            let og_buf_len = buf.len();
            // 
            for i in 0..og_buf_len {
                let msg = buf[i].clone();
                write_msg(&msg, &file).await.unwrap();
            }

            buf.clear();

        }
    });

    #[cfg(feature = "logging")]
    let mut last_msg = SystemTime::now();
    let mut first = true;
    'pass: loop {
        // This is Some if some message needs to be inserted before everything else
        // This is used when it sends the dropped messsages message and the first response is the first
        let mut first_data: Option<([u8; 508], usize)> = None;

        if !first {
            // Get unrecieved messages from progress tracker
            let dropped = prog_tracker.get_unrecv().await?;

            // If there were no dropped messages
            if dropped.len() == 0 {
                // Everything was recieved correctly
                #[cfg(feature = "logging")]
                debug!("everything recieved correctly");

                loop {
                    let sleep = time::sleep(Duration::from_millis(1500));

                    let mut buf = [0u8; 508];
                    tokio::select! {
                        _ = sleep => {
                            break;
                        }

                        amt = recv(&sock, &sender, &mut buf) => {
                            let amt = amt?;
                            let buf = &buf[0..amt];

                            // Make sure the sender knows the file is recieved
                            if buf[0] == 5 {
                                sock.send_to(&[7], sender).await?;

                            }

                        }
                    }
                }

                break;
            }
            // Everything was not sent correctly

            #[cfg(feature = "logging")]
            debug!("everything was not recieved correctly");

            // Generate message containing dropped messages id
            let dropped_msg = gen_dropped_msg(dropped)?;

            loop {
                // Send dropped messages to sender
                let mut buf = [0u8; 508];
                let amt = send_until_recv(&sock, &dropped_msg, &sender, &mut buf, 10).await?;

                // This will probably be the first data msg
                let msg_buf = &buf[0..amt];
                if msg_buf.len() > 1 {
                    // This message will be the first i a sequence of messages
                    // That's why we use first data
                    first_data = Some((buf, amt));
                    break;
                }
            }
        }

        loop {
            // TODO: Writing the message takes too much time
            // It can be fixed by writing to a buffer in memory and writing that buffer not that often
            // Get message from sender
            let mut buf = [0; 508];
            let amt = if let Some((new_buf, amt)) = first_data {
                // If there already was data then it should use it
                buf = new_buf;

                first_data = None;
                #[cfg(feature = "logging")]
                debug!("the msg recieved was first data");

                amt
            } else {
                // Otherwise it should reiceve from the sender
                // Recieve message from sender

                #[cfg(feature = "logging")]
                {
                    let last_msg_u64 = last_msg.duration_since(UNIX_EPOCH).unwrap().as_secs_f64();

                    let now = SystemTime::now();
                    let now_f64 = now.duration_since(UNIX_EPOCH).unwrap().as_secs_f64();

                    debug!("last updated {}s ago", now_f64 - last_msg_u64);
                }

                let wait_time = time::sleep(Duration::from_millis(2000));
                let amt = tokio::select! {
                    _ = wait_time => {
                        break;
                    }

                    amt = recv(&sock, &sender, &mut buf) => {
                        let amt = amt?;
                        amt
                    }
                };

                #[cfg(feature = "logging")]
                {
                    let now = SystemTime::now();

                    last_msg = now;
                }

                amt
            };

            // Create a slice of the buffer
            let buf = &buf[0..amt];

            // Skip if it's just a hole punch msg
            if buf.len() == 1 && buf[0] == 255 {
                continue;
            } else if buf.len() == 1 && buf[0] == 5 {
                // Done sending
                continue 'pass;
                // This will send the dropped messages in the new pass
            } else if first && buf[0] == 8 {
                continue;
            }

            // Write the msg to the disk
            // Remember msg num if logging is on
            // This is to log progress
            let msg_num = u8s_to_u64(&buf[0..8])?;
            // Append message to buffer
            msg_buffer.write().await.push(buf.to_owned());
            prog_tracker.recv_msg(msg_num).await?;

            // Main thread handles progress tracking
            // msg buffer writes messages

            #[cfg(feature = "logging")]
            info!(
                "msg {} / {}, {}%",
                msg_num,
                size / 500,
                msg_num * 100 / (size / 500)
            );
            first = false;
        }
    }
    holepuncher.abort();
    prog_tracker.destruct().await;

    time::sleep(Duration::from_millis(1000)).await;
    msg_writer.abort();

    Ok(())
}

/// Converts an array of dropped messages into a 'dropped messages' message
fn gen_dropped_msg(dropped: Vec<u64>) -> Result<Vec<u8>, Box<dyn error::Error + Send + Sync>> {
    if dropped.len() > 63 {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "maximum amount of dropped messages is 63, got {}",
                dropped.len()
            )
            .as_str(),
        )));
    }

    let mut msg: Vec<u8> = vec![6];
    for drop in dropped {
        msg.append(&mut drop.to_be_bytes().as_slice().to_owned())
    }

    Ok(msg)
}

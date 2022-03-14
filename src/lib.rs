//! ## Usage
//! \
//!
//! **Sender example**
//!
//! ```
//! let port = 3456;
//! println!("my ip: 127.0.0.1:{}", port);
//!
//! // Send the file with the send_file funciton
//! send_file(
//!     Source::Port(port),
//!     "./examples/file_to_send.txt",
//!     "127.0.0.1:7890",
//! )
//! .await
//! .expect("error when sending file");
//! ```
//! \
//!
//! **Reciever example**
//!
//! ```
//! let port = 7890;
//! println!("my ip: 127.0.0.1:{}", port);
//!
//! recv_file(
//!     Source::Port(port),
//!     &mut File::create("output_from_recv.txt").expect("could not create output file"),
//!     "127.0.0.1:3456",
//!     ProgressTracking::Memory,
//! )
//! .await
//! .expect("error when sending file");
//! ```
//!
//! ## Sending files
//!
//! ### Messages
//!
//! Messages are 508 bytes in size. This is because that is the biggest message you can send over
//! udp without getting dropped.
//!
//! The first 8 bytes or 64 bits in the message are used for telling what message this is.
//! The counting starts at 0.
//!
//! The rest is content of the file
//!
//! ### Hole punch
//!
//! A hole punch is a way for clients to communicate without requireing a port-forward.
//!
//! [Here](https://en.wikipedia.org/wiki/UDP_hole_punching) is a wikipedia article about it.
//!
//! But it works like this
//! 1. Client A sends a udp message to client B:s ip-address and port.
//! 2. Client B does the same as client A but with client A:s ip-address and port.
//! 3. Now they are able to send messages over udp from where they have hole-punched to.
//!
//! ### Progress tracking
//!
//! When recieving the reciever remembers what messages have been sent.
//! It also knows how many that should be sent.
//! With that info it can tell the sender what messages are unsent.
//!
//! The progress tracking can be stored in memory and on file if the messages are to many.
//!
//! You can **calculate** the memory required for recieving a file, file size / 500 / 8 = size of progress tracker.
//!
//! **Example**: 64gb = 64 000 000 000 / 500 / 8 = 16 000 000 = 16mb
//!
//! The progress tracker for 64gb is 16mb.
//!
//! ### Sending
//!
//! It sends the file by sending many messages.
//! When it's done it will send a message telling the reciever that it's done.
//! If any messages got dropped the reciever will send a list of those.
//! If the file was recieved correctly the reciever will send a message.
//!

use std::{
    error,
    error::Error,
    fs::File,
    io::{self},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use tokio::{
    net::{lookup_host, ToSocketAddrs, UdpSocket},
    time,
};

#[cfg(target_os = "linux")]
use std::os::unix::fs::FileExt;
#[cfg(target_os = "windows")]
use std::os::windows::prelude::FileExt;

pub mod reciever;
pub mod sender;

pub use reciever::recv_file;
pub use sender::send_file;

/// done_sending: 5
/// missed_messages: 6
/// all_messages are sent: 7
/// file_size: 8
/// send_file_size: 9
///
/// holepunch: 255
// TODO: Use this
enum _Message {
    DoneSending,
    MissedMessages(Vec<u64>),
    FileRecieved,
    FileSize(u64),
    SendFileSize,
    HolePunch,
}

fn u8s_to_u64(nums: &[u8]) -> io::Result<u64> {
    if nums.len() != 8 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nums must be 8 bytes long",
        ));
    }
    let msg_u8: [u8; 8] = [
        nums[0], nums[1], nums[2], nums[3], nums[4], nums[5], nums[6], nums[7],
    ];

    let big_number = u64::from_be_bytes(msg_u8);
    Ok(big_number)
}

async fn send_unil_recv<T: ToSocketAddrs>(
    sock: &UdpSocket,
    msg: &[u8],
    addr: &T,
    buf: &mut [u8],
    interval: u64,
) -> Result<usize, Box<dyn error::Error>> {
    let mut send_interval = time::interval(Duration::from_millis(interval));
    let amt = loop {
        tokio::select! {
            _ = send_interval.tick() => {
                sock.send_to(msg, addr).await?;

            }

            result = sock.recv_from(buf) => {
                let (amt, src) = result?;
                if &src != &lookup_host(addr).await?.next().unwrap() {
                    continue;
                }
                break amt;
            }
        }
    };

    Ok(amt)
}

async fn punch_hole<T: ToSocketAddrs>(sock: &UdpSocket, addr: T) -> Result<(), Box<dyn Error>> {
    sock.send_to(&[255u8], addr).await?;

    Ok(())
}

fn get_buf(msg_num: &u64, file_buf: &[u8]) -> Vec<u8> {
    let msg_num_u8 = msg_num.to_be_bytes();

    let full = [&msg_num_u8, file_buf].concat();

    full
}

fn read_position(file: &File, buf: &mut [u8], offset: u64) -> Result<usize, Box<dyn error::Error>> {
    #[cfg(target_os = "linux")]
    let amt = file.read_at(buf, offset)?;

    #[cfg(target_os = "windows")]
    let amt = file.seek_read(buf, offset)?;

    Ok(amt)
}

fn write_position(file: &File, buf: &[u8], offset: u64) -> Result<usize, Box<dyn error::Error>> {
    #[cfg(target_os = "linux")]
    let amt = file.write_at(&buf, offset)?;

    #[cfg(target_os = "windows")]
    let amt = file.seek_write(&buf, offset)?;

    Ok(amt)
}

async fn recv<T>(sock: &UdpSocket, from: &T, buf: &mut [u8]) -> Result<usize, Box<dyn Error>>
where
    T: ToSocketAddrs,
{
    loop {
        let (amt, src) = sock.recv_from(buf).await?;

        if &src == &lookup_host(from).await?.next().unwrap() {
            return Ok(amt);
        }
    }
}

pub enum Source {
    SocketArc(Arc<UdpSocket>),
    Socket(UdpSocket),
    Port(u16),
}

impl Source {
    async fn into_socket(self) -> Arc<UdpSocket> {
        match self {
            Source::SocketArc(s) => s,
            Source::Socket(s) => Arc::new(s),
            Source::Port(port) => Arc::new(
                UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port))
                    .await
                    .unwrap(),
            ),
        }
    }
}

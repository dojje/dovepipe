//! Used to send file
//!
//! # Sending files
//!
//! ## Messages
//!
//! Messages are 508 bytes in size. This is because that is the biggest message you can send over
//! udp without getting dropped.
//!
//! The first 8 bytes or 64 bits in the message are used for telling what message this is.
//! The counting starts at 0.
//!
//! The rest is content of the file
//!
//! ## Hole punch
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
//! ## Sending
//!
//! It sends the file by sending many messages. When it's done it will send a message.
//! If any messages got dropped the reciever will send a list of those.
//! If the file was recieved correctly the reciever will send a message.
//!

use std::{
    error,
    error::Error,
    fs::File,
    io::{self},
    net::SocketAddr,
    time::Duration,
};

use tokio::{net::UdpSocket, time};

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

async fn send_unil_recv(
    sock: &UdpSocket,
    msg: &[u8],
    addr: &SocketAddr,
    buf: &mut [u8],
    interval: u64,
) -> Result<usize, Box<dyn error::Error>> {
    let mut send_interval = time::interval(Duration::from_millis(interval));
    let amt = loop {
        tokio::select! {
            _ = send_interval.tick() => {
                #[cfg(feature = "sim_wan")]
                send_maybe(&sock, msg, addr).await?;
                #[cfg(not(feature = "sim_wan"))]
                sock.send_to(msg, addr).await?;

            }

            result = sock.recv_from(buf) => {
                let (amt, src) = result?;
                if &src != addr {
                    continue;
                }
                break amt;
            }
        }
    };

    Ok(amt)
}

async fn punch_hole(sock: &UdpSocket, addr: SocketAddr) -> Result<(), Box<dyn Error>> {
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

async fn recv(
    sock: &UdpSocket,
    from: &SocketAddr,
    buf: &mut [u8],
) -> Result<usize, Box<dyn Error>> {
    loop {
        let (amt, src) = sock.recv_from(buf).await?;

        if &src == from {
            return Ok(amt);
        }
    }
}

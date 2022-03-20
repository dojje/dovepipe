use std::{io, net::SocketAddr, str::FromStr};
use tokio::fs::File;

use dovepipe::{reciever::ProgressTracking, recv_file, Source};

#[tokio::main]
async fn main() {
    let port = 7890;
    println!("my ip: 127.0.0.1:{}", port);
    println!("Enter ip address and port of sender:");
    let mut reciever_ip_str = String::new();
    io::stdin()
        .read_line(&mut reciever_ip_str)
        .expect("Failed to read input");

    let reciever_ip_str = reciever_ip_str[0..reciever_ip_str.len() - 2].to_string();

    let reciever = SocketAddr::from_str(&reciever_ip_str).expect("not a valid ip address");

    recv_file(
        Source::Port(7890),
        &mut File::create("output_from_recv.txt").await.expect("could not create output file"),
        reciever,
        ProgressTracking::Memory,
    )
    .await
    .expect("error when sending file");
}

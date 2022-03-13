use std::{fs::File, io, net::SocketAddr, str::FromStr};

use dovepipe::{reciever::ProgressTracking, recv_file};

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
        &mut File::create("output_from_recv.txt").expect("could not create output file"),
        7890,
        reciever,
        ProgressTracking::Memory,
    )
    .await
    .expect("error when sending file");
}

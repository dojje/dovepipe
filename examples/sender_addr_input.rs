use std::{io, net::SocketAddr, str::FromStr};

use dovepipe::{send_file, Source};

#[tokio::main]
async fn main() {
    let port = 3456;
    println!("my ip: 127.0.0.1:{}", port);

    // Read the ip of the reciever from stdin
    println!("Enter ip address and port of reciever: ");
    let mut reciever_ip_str = String::new();
    io::stdin()
        .read_line(&mut reciever_ip_str)
        .expect("Failed to read input");

    let reciever_ip_str = reciever_ip_str[0..reciever_ip_str.len() - 2].to_string();

    // Create a udp socket

    let reciever = SocketAddr::from_str(&reciever_ip_str).expect("not a valid ip address");

    // Send the file with the send_file funciton
    send_file(Source::Port(3456), "./examples/file_to_send.txt", reciever)
        .await
        .expect("error when sending file");
}

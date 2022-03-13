use std::{io, net::SocketAddr, str::FromStr, sync::Arc};

use dovepipe::send_file;
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() {
    let my_ip = "0.0.0.0:3456";
    println!("my ip: {}", my_ip);

    // Read the ip of the reciever from stdin
    println!("Enter ip address and port of reciever: ");
    let mut reciever_ip_str = String::new();
    io::stdin()
        .read_line(&mut reciever_ip_str)
        .expect("Failed to read input");

    let reciever_ip_str = reciever_ip_str[0..reciever_ip_str.len()-2].to_string();

    // Create a udp socket

    let reciever = SocketAddr::from_str(&reciever_ip_str).expect("not a valid ip address");


    let sock = UdpSocket::bind(my_ip)
        .await
        .expect("could not bind to address");


    // Send the file with the send_file funciton
    send_file(Arc::new(sock), "./examples/file_to_send.txt", reciever, false)
        .await
        .expect("error when sending file");

}

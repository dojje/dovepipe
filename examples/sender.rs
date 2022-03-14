use dovepipe::{Source, send_file};

#[tokio::main]
async fn main() {
    let port = 3456;
    println!("my ip: 127.0.0.1:{}", port);

    // Send the file with the send_file funciton
    send_file(
        Source::Port(port),
        "./examples/file_to_send.txt",
        "127.0.0.1:7890",
    )
    .await
    .expect("error when sending file");
}

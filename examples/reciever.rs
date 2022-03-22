use std::path::Path;



use dovepipe::{reciever::ProgressTracking, recv_file, Source};

#[tokio::main]
async fn main() {
    let port = 7890;
    println!("my ip: 127.0.0.1:{}", port);

    recv_file(
        Source::Port(port),
        &Path::new("output_from_recv.txt"),
        "127.0.0.1:3456",
        ProgressTracking::Memory,
    )
    .await
    .expect("error when sending file");
}

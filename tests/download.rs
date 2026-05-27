use std::path::Path;

use bilidown::{client::BiliClient, download::download_stream};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test]
async fn resumes_partial_tmp_file_with_range_request() {
    let tmpdir = tempfile::tempdir().unwrap();
    let dest = tmpdir.path().join("out.bin");
    let tmp = dest.with_extension("bin.tmp");
    tokio::fs::write(&tmp, b"hello").await.unwrap();

    let url = spawn_range_server().await;
    let client = BiliClient::new().unwrap();

    download_stream(&client, &url, &dest, "test").await.unwrap();

    assert_eq!(tokio::fs::read(&dest).await.unwrap(), b"hello world");
    assert!(!Path::new(&tmp).exists());
}

async fn spawn_range_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0_u8; 4096];
        let n = socket.read(&mut buf).await.unwrap();
        let req = String::from_utf8_lossy(&buf[..n]);
        assert!(
            req.to_ascii_lowercase().contains("range: bytes=5-"),
            "{req}"
        );

        let response = concat!(
            "HTTP/1.1 206 Partial Content\r\n",
            "Content-Length: 6\r\n",
            "Content-Range: bytes 5-10/11\r\n",
            "Connection: close\r\n",
            "\r\n",
            " world"
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    format!("http://{addr}/file")
}

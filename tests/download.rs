use std::path::Path;

use bilidown::{
    client::BiliClient,
    download::{DownloadConfig, download_stream, download_stream_with_urls},
};
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

#[tokio::test]
async fn falls_back_to_backup_url_after_failed_primary() {
    let tmpdir = tempfile::tempdir().unwrap();
    let dest = tmpdir.path().join("fallback.bin");
    let bad = spawn_status_server(403, b"forbidden").await;
    let good = spawn_status_server(200, b"ok").await;
    let client = BiliClient::new().unwrap();

    download_stream_with_urls(
        &client,
        &[bad, good],
        &dest,
        "fallback",
        DownloadConfig {
            connections: 1,
            retries: 1,
        },
    )
    .await
    .unwrap();

    assert_eq!(tokio::fs::read(&dest).await.unwrap(), b"ok");
}

#[tokio::test]
async fn downloads_file_using_parallel_range_parts() {
    let tmpdir = tempfile::tempdir().unwrap();
    let dest = tmpdir.path().join("parallel.bin");
    let body = b"abcdefghijklmnopqrstuvwxyz".to_vec();
    let url = spawn_multi_range_server(body.clone(), 5).await;
    let client = BiliClient::new().unwrap();

    download_stream_with_urls(
        &client,
        &[url],
        &dest,
        "parallel",
        DownloadConfig {
            connections: 4,
            retries: 1,
        },
    )
    .await
    .unwrap();

    assert_eq!(tokio::fs::read(&dest).await.unwrap(), body);
}

async fn spawn_status_server(status: u16, body: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0_u8; 4096];
        let _ = socket.read(&mut buf).await.unwrap();
        let status_text = if status == 200 { "OK" } else { "Forbidden" };
        let response = format!(
            "HTTP/1.1 {status} {status_text}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.write_all(body).await.unwrap();
    });
    format!("http://{addr}/file")
}

async fn spawn_multi_range_server(body: Vec<u8>, expected_requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        for _ in 0..expected_requests {
            let (mut socket, _) = listener.accept().await.unwrap();
            let body = body.clone();
            tokio::spawn(async move {
                let mut buf = [0_u8; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
                assert!(
                    req.contains("accept-encoding: identity"),
                    "download requests should disable transparent decompression: {req}"
                );
                if let Some(range) = parse_range_header(&req) {
                    let (start, end) = range;
                    let end = end.min(body.len() - 1);
                    let chunk = &body[start..=end];
                    let response = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nConnection: close\r\n\r\n",
                        chunk.len(),
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                    socket.write_all(chunk).await.unwrap();
                } else {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                    socket.write_all(&body).await.unwrap();
                }
            });
        }
    });
    format!("http://{addr}/file")
}

fn parse_range_header(req: &str) -> Option<(usize, usize)> {
    let range = req
        .lines()
        .find_map(|line| line.strip_prefix("range: bytes="))?;
    let (start, end) = range.split_once('-')?;
    Some((start.trim().parse().ok()?, end.trim().parse().ok()?))
}

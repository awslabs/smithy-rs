/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use std::net::SocketAddr;
use std::time::Duration;
use aws_sdk_s3::{
    Credentials, Region,
    Endpoint
};
use bytes::BytesMut;
use tracing::debug;

// static INIT_LOGGER: std::sync::Once = std::sync::Once::new();
//
// fn init_logger() {
//     INIT_LOGGER.call_once(|| {
//         tracing_subscriber::fmt::init();
//     });
// }

// test will hang forever with the default (single-threaded) test executor
#[tokio::test(flavor = "multi_thread")]
#[should_panic(expected = "error reading a body from connection: end of file before message length reached")]
async fn test_streaming_response_fails_when_eof_comes_before_content_length_reached() {
    // init_logger();

    let addr = SocketAddr::from(([0,0,0,0], 3000));
    // We spawn a faulty server that will close the connection after
    // writing half of the response body.
    let _server = tokio::spawn(start_faulty_server(addr));

    let creds = Credentials::new(
        "ANOTREAL",
        "notrealrnrELgWzOk3IfjzDKtFBhDby",
        Some("notarealsessiontoken".to_string()),
        None,
        "test",
    );

    let conf = aws_sdk_s3::Config::builder()
        .credentials_provider(creds)
        .region(Region::new("us-east-1"))
        .endpoint_resolver(
            Endpoint::immutable("http://localhost:3000".parse().expect("valid URI"))
        )
        .build();

    let client = aws_sdk_s3::client::Client::from_conf(conf);

    // This will succeed b/c the head of the response is fine.
    let res = client
        .get_object()
        .bucket("some-test-bucket")
        .key("test.txt")
        .send()
        .await
        .unwrap();

    // Should panic here when the body is read with an "UnexpectedEof" error
    if let Err(e) = res.body.collect().await {
        panic!("{e}")
    }
}

async fn start_faulty_server(addr: SocketAddr) {
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::sleep;

    let listener = TcpListener::bind(addr).await.expect("socket is free");

    async fn process_socket(socket: TcpStream) {
        let mut buf = BytesMut::new();
        let response: &[u8] = br#"HTTP/1.1 200 OK
x-amz-request-id: 4B4NGF0EAWN0GE63
content-length: 12
etag: 3e25960a79dbc69b674cd4ec67a72c62
content-type: application/octet-stream
server: AmazonS3
content-encoding:
last-modified: Tue, 21 Jun 2022 16:29:14 GMT
date: Tue, 21 Jun 2022 16:29:23 GMT
x-amz-id-2: kPl+IVVZAwsN8ePUyQJZ40WD9dzaqtr4eNESArqE68GSKtVvuvCTDe+SxhTT+JTUqXB1HL4OxNM=
accept-ranges: bytes

Hello"#;
        let mut nothing_more_to_read = false;
        let mut time_to_respond = false;

        loop {
            match socket.try_read_buf(&mut buf) {
                Ok(0) => {
                    debug!("stream read has been closed, breaking from loop");
                    nothing_more_to_read = true;
                }
                Ok(n) => {
                    debug!("read {n} bytes from the socket reader");
                    if let Ok(s) = std::str::from_utf8(&buf) {
                        debug!("buf currently looks like:\n{s:?}");
                    }

                    if buf.ends_with(b"\r\n\r\n") {
                        time_to_respond = true;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    debug!("would block, looping again after small 1ms nap");
                    sleep(Duration::from_millis(1)).await;
                }
                Err(e) => {
                    panic!("{e}")
                }
            }

            if nothing_more_to_read {
                let s = std::str::from_utf8(&buf).unwrap();
                debug!("nothing_more_to_read, server received {s}");
                break;
            }

            if socket.writable().await.is_ok() {
                if time_to_respond {
                    // The content length is 12 but we'll only write 5 bytes
                    socket.try_write(&response).unwrap();
                    // We break from the R/W loop after sending a partial response in order to
                    // close the connection early.
                    debug!("faulty server has written partial response, now closing connection");
                    break;
                }
            }
        }
    }

    loop {
        let (socket, addr) = listener.accept().await.expect("listener can accept new connections");
        debug!("server received new connection from {addr:?}");
        let start = std::time::Instant::now();
        process_socket(socket).await;
        debug!("connection to {addr:?} closed after {:.02?}", start.elapsed());
    }
}


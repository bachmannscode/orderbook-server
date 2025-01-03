use orderbook_server::*;
use std::io::Write;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{self, sleep, timeout};

struct WriteSender(Sender<u8>);

impl Write for WriteSender {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &byte in buf {
            self.0.send(byte).unwrap()
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn broadcast_server() {
    // Setup
    let (tx, rx) = std::sync::mpsc::channel();
    init_logger(
        "info".into(),
        env_logger::Target::Pipe(Box::new(WriteSender(tx))),
    );
    let mut server_addr = "127.0.0.1:8888".to_string();
    tokio::spawn(run(server_addr.as_str().parse().unwrap()));
    time::sleep(Duration::from_millis(50)).await;

    // Assert the initial server log.
    let initial_server_log = String::from_utf8(rx.try_iter().collect::<Vec<u8>>()).unwrap();
    if "listening on port 8888\n" != initial_server_log {
        // If this is entered, then the first server log included the fallback message.
        let listening_log = initial_server_log
            .split('.')
            .last()
            .unwrap()
            .strip_prefix("\n")
            .unwrap();
        let port = listening_log
            .split(' ')
            .last()
            .unwrap()
            .strip_suffix("\n")
            .unwrap();
        server_addr = format!("127.0.0.1:{port}");
        assert_eq!(format!("listening on port {port}\n"), listening_log);
    }

    // Connect 3 clients and assert client output and server logs.
    let (alice_reader, mut alice_writer) = timeout(Duration::from_secs(2), async {
        loop {
            match TcpStream::connect(&server_addr).await {
                Ok(a) => break a,
                Err(_) => {
                    sleep(Duration::from_millis(50)).await;
                }
            }
        }
    })
    .await
    .unwrap()
    .into_split();
    let alice_port = alice_reader.local_addr().unwrap().port();
    let mut alice_lines = BufReader::new(alice_reader).lines();
    sleep(Duration::from_millis(50)).await;
    assert_eq!(
        &format!("connected 127.0.0.1 {alice_port}\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    let (bob_reader, mut bob_writer) = TcpStream::connect(&server_addr).await.unwrap().into_split();
    let bob_port = bob_reader.local_addr().unwrap().port();
    let mut bob_lines = BufReader::new(bob_reader).lines();
    sleep(Duration::from_millis(50)).await;
    assert_eq!(
        &format!("connected 127.0.0.1 {bob_port}\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    let john = TcpStream::connect(&server_addr).await.unwrap();
    let john_port = john.local_addr().unwrap().port();
    let mut john_lines = BufReader::new(john).lines();
    sleep(Duration::from_millis(50)).await;
    assert_eq!(
        &format!("connected 127.0.0.1 {john_port}\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );

    // Send buy orders and assert client output and server log.
    // APPLE
    alice_writer.write_all(b"BUY:APPLE\n").await.unwrap();
    assert_eq!("ACK:APPLE", alice_lines.next_line().await.unwrap().unwrap());
    assert_eq!(
        &format!("new buy order {alice_port} APPLE\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    alice_writer.write_all(b"BUY:APPLE\n").await.unwrap();
    assert_eq!("ACK:APPLE", alice_lines.next_line().await.unwrap().unwrap());
    assert_eq!(
        &format!("new buy order {alice_port} APPLE\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    // PEAR
    alice_writer.write_all(b"BUY:PEAR\n").await.unwrap();
    assert_eq!("ACK:PEAR", alice_lines.next_line().await.unwrap().unwrap());
    assert_eq!(
        &format!("new buy order {alice_port} PEAR\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    // TOMATO
    alice_writer.write_all(b"BUY:TOMATO\n").await.unwrap();
    assert_eq!(
        "ACK:TOMATO",
        alice_lines.next_line().await.unwrap().unwrap()
    );
    assert_eq!(
        &format!("new buy order {alice_port} TOMATO\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    // POTATO
    alice_writer.write_all(b"BUY:POTATO\n").await.unwrap();
    assert_eq!(
        "ACK:POTATO",
        alice_lines.next_line().await.unwrap().unwrap()
    );
    assert_eq!(
        &format!("new buy order {alice_port} POTATO\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );

    // Send sell orders and assert client output and server log.
    // "first buy then sell" trade happens.
    bob_writer.write_all(b"SELL:ONION\n").await.unwrap();
    assert_eq!("ACK:ONION", bob_lines.next_line().await.unwrap().unwrap());
    assert_eq!(
        &format!("new sell order {bob_port} ONION\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    bob_writer.write_all(b"SELL:APPLE\n").await.unwrap();
    assert_eq!("ACK:APPLE", bob_lines.next_line().await.unwrap().unwrap());
    assert_eq!(
        &format!("new sell order {bob_port} APPLE\ntrade APPLE\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    assert_eq!("TRADE:APPLE", bob_lines.next_line().await.unwrap().unwrap());
    assert_eq!(
        "TRADE:APPLE",
        alice_lines.next_line().await.unwrap().unwrap()
    );
    assert_eq!(
        "TRADE:APPLE",
        john_lines.next_line().await.unwrap().unwrap()
    );

    // Assert "first sell then buy" trade.
    alice_writer.write_all(b"BUY:ONION\n").await.unwrap();
    assert_eq!("ACK:ONION", alice_lines.next_line().await.unwrap().unwrap());
    assert_eq!(
        &format!("new buy order {alice_port} ONION\ntrade ONION\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    assert_eq!("TRADE:ONION", bob_lines.next_line().await.unwrap().unwrap());
    assert_eq!(
        "TRADE:ONION",
        alice_lines.next_line().await.unwrap().unwrap()
    );
    assert_eq!(
        "TRADE:ONION",
        john_lines.next_line().await.unwrap().unwrap()
    );

    // Drop the initial client, assert the server log and verify that the corresponding
    // server channel is dropped correctly without causing a crash on the next message.
    drop(alice_writer);
    time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        &format!("disconnected 127.0.0.1 {alice_port}\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    bob_writer.write_all(b"BUY:TOMATO\n").await.unwrap();
    assert_eq!("ACK:TOMATO", bob_lines.next_line().await.unwrap().unwrap());
    assert_eq!(
        &format!("new buy order {bob_port} TOMATO\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );

    // Assert invalid input error messages.
    bob_writer.write_all(b"asdf:TOMATO\n").await.unwrap();
    assert_eq!(
        "Operation not supported.",
        bob_lines.next_line().await.unwrap().unwrap()
    );
    bob_writer.write_all(b"BUY:asdf\n").await.unwrap();
    assert_eq!(
        "Commodity not supported.",
        bob_lines.next_line().await.unwrap().unwrap()
    );
    bob_writer.write_all(b"BUYTOMATO\n").await.unwrap();
    assert_eq!(
        "Invalid order command.",
        bob_lines.next_line().await.unwrap().unwrap()
    );
    bob_writer.write_all(b"BUY:TOMATO:APPLE\n").await.unwrap();
    assert_eq!(
        "Invalid order command.",
        bob_lines.next_line().await.unwrap().unwrap()
    );

    // Assert the drop server logs of the remaining clients.
    drop(bob_writer);
    time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        &format!("disconnected 127.0.0.1 {bob_port}\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
    drop(john_lines);
    time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        &format!("disconnected 127.0.0.1 {john_port}\n"),
        std::str::from_utf8(&rx.try_iter().collect::<Vec<u8>>()).unwrap()
    );
}

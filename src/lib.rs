use env_logger::Target;
use std::collections::HashMap;
use std::fmt::Display;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc};

pub fn init_logger(logging_level: String, target: Target) {
    std::env::set_var("RUST_LOG", logging_level);
    env_logger::Builder::new()
        .parse_default_env()
        .format_timestamp(None)
        .format_level(false)
        .format_target(false)
        .target(target)
        .init();
}

#[derive(Clone, Debug)]
struct Message {
    commodity: Commodity,
    operation: Operation,
}

impl FromStr for Message {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut s = s.split(':');
        match (s.next(), s.next(), s.next()) {
            (Some(op), Some(cm), None) if !op.is_empty() && !cm.is_empty() => Ok(Self {
                operation: op.parse()?,
                commodity: cm.parse()?,
            }),
            _ => Err("Invalid order command."),
        }
    }
}

#[derive(Hash, PartialEq, Eq, Debug, Clone, Copy)]
enum Commodity {
    Apple,
    Pear,
    Tomato,
    Potato,
    Onion,
}

impl FromStr for Commodity {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "APPLE" => Ok(Self::Apple),
            "PEAR" => Ok(Self::Pear),
            "TOMATO" => Ok(Self::Tomato),
            "POTATO" => Ok(Self::Potato),
            "ONION" => Ok(Self::Onion),
            _ => Err("Commodity not supported."),
        }
    }
}

impl Display for Commodity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Self::Apple => write!(f, "APPLE"),
            Self::Pear => write!(f, "PEAR"),
            Self::Tomato => write!(f, "TOMATO"),
            Self::Potato => write!(f, "POTATO"),
            Self::Onion => write!(f, "ONION"),
        }
    }
}

#[derive(Debug, Clone)]
enum Operation {
    Buy,
    Sell,
}

impl FromStr for Operation {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "BUY" => Ok(Self::Buy),
            "SELL" => Ok(Self::Sell),
            _ => Err("Operation not supported."),
        }
    }
}

impl Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Self::Buy => write!(f, "buy"),
            Self::Sell => write!(f, "sell"),
        }
    }
}

// The hashmap could hypothetically be replaced by an array in this simple implementation.
struct OrderBook(HashMap<Commodity, i32>);

impl OrderBook {
    fn new() -> Self {
        Self(HashMap::new())
    }

    // returns true if a trade happens
    fn add_buy_order(&mut self, commodity: Commodity) -> bool {
        let orders = self.0.entry(commodity).or_insert(0);
        *orders += 1;
        *orders <= 0
    }

    // returns true if a trade happens
    fn add_sell_order(&mut self, commodity: Commodity) -> bool {
        let orders = self.0.entry(commodity).or_insert(0);
        *orders -= 1;
        *orders >= 0
    }
}

async fn handle_orderbook(
    mut orderbook: OrderBook,
    mut rx: mpsc::Receiver<Message>,
    confirm_tx: broadcast::Sender<Commodity>,
) {
    while let Some(msg) = rx.recv().await {
        if match msg {
            Message {
                commodity,
                operation: Operation::Buy,
            } => orderbook.add_buy_order(commodity),
            Message {
                commodity,
                operation: Operation::Sell,
            } => orderbook.add_sell_order(commodity),
        } {
            log::info!("trade {}", msg.commodity);
            let _ = confirm_tx.send(msg.commodity);
        }
    }
}

async fn handle_connection(
    socket: TcpStream,
    addr: SocketAddr,
    tx: mpsc::Sender<Message>,
    mut confirm_rx: broadcast::Receiver<Commodity>,
) {
    let port = addr.port();
    log::info!("connected {} {}", addr.ip(), port);
    let (reader, mut writer) = socket.into_split();
    let mut lines = BufReader::new(reader).lines();
    loop {
        tokio::select! {
            msg = lines.next_line() => {
                match msg {
                    Ok(Some(msg)) => {
                        let msg: Result<Message, &'static str> = msg.parse();
                        match msg {
                            Err(e) => {
                                let _ = writer.write_all(format!("{e}\n").as_bytes()).await;
                            }
                            Ok(msg) => {
                                log::info!("new {} order {port} {}", msg.operation, msg.commodity);
                                let _ = tx
                                    .send(Message {
                                        operation: msg.operation,
                                        commodity: msg.commodity,
                                    })
                                    .await;
                                let _ = writer
                                    .write_all(format!("ACK:{}\n", msg.commodity).as_bytes())
                                    .await;
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                        log::info!("disconnected {} {port}", addr.ip());
                        return;
                    }
                    Ok(None) => {
                        log::info!("disconnected {} {port}", addr.ip());
                        return;
                    }
                    _ => {
                        return;
                    }
                }
            },
            commodity = confirm_rx.recv() => {
                match commodity {
                    Ok(c) => {
                        let _ = writer.write_all(format!("TRADE:{c}\n").as_bytes()).await;
                    },
                    Err(RecvError::Lagged(n)) => {
                        log::warn!("{n} messages to port {port} omitted due to lagging receiver.");
                    },
                    _ => {
                        return;
                    }
                }
            }
        }
    }
}

/// An Orderbook that registers buy and sell orders and notifies for trades.
pub async fn run(addr: SocketAddr) {
    let mut port = addr.port();
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(_) => {
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            port = l.local_addr().unwrap().port();
            log::warn!("Not able to use {addr}, fallback to 127.0.0.1:{port}.");
            l
        }
    };
    log::info!("listening on port {port}");
    let (tx, rx) = mpsc::channel::<Message>(16);
    let (confirm_tx, _) = broadcast::channel::<Commodity>(16);
    let orderbook = OrderBook::new();
    tokio::spawn(handle_orderbook(orderbook, rx, confirm_tx.clone()));
    while let Ok((socket, peer_addr)) = listener.accept().await {
        tokio::spawn(handle_connection(
            socket,
            peer_addr,
            tx.clone(),
            confirm_tx.subscribe(),
        ));
    }
}

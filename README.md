# Orderbook Server

A simple single-threaded asynchronous TCP order book server implemented in Rust using `tokio`. This server allows clients to place buy and sell orders for commodities, matching them automatically and notifying all connected clients when trades occur. It can be tested using `netcat` (`nc`).

## Features

- Handles multiple clients asynchronously
- Uses a single-threaded asynchronous approach
- Supports buy and sell orders for predefined commodities
- Matches buy and sell orders in real-time
- Notifies all clients when a trade is executed
- Messages are delimited by newlines (`\n`)
- Simple logging for connection and order tracking

## Usage

### Running the Server

The server listens for incoming TCP connections and processes buy and sell orders. You can start the server simply with:

```sh
cargo run
```

or choose your own port with:
```sh
cargo run -- --socket 127.0.0.1:[PORT]
```

or by choosing a logging level for example only log errors with:
```sh
cargo run -- --logging-level error
```

### Connecting Clients with `netcat`

You can connect to the server using `nc` and place orders:

```sh
nc localhost 8888
```

Upon connection, the server logs the new client connection. Clients can submit orders using the following format:

```
BUY:[COMMODITY]
SELL:[COMMODITY]
```
Supported commodities:
- APPLE
- PEAR
- TOMATO
- POTATO
- ONION

The server acknowledges valid orders:

```
ACK:[COMMODITY]
```

If a matching order exists, a trade is executed and all connected clients receive a trade notification:

```
TRADE:[COMMODITY]
```

### Example Session

#### Terminal 1 (Server)
```sh
cargo run --release -- --socket 127.0.0.1:8888
```
```
listening on port 8888
connected 127.0.0.1 25565
connected 127.0.0.1 41337
new buy order 25565 APPLE
new sell order 41337 APPLE
trade APPLE
```

#### Terminal 2 (Client Alice - Buyer)
```sh
nc localhost 8888
```
```
BUY:APPLE
ACK:APPLE
```

#### Terminal 3 (Client Bob - Seller)
```sh
nc localhost 8888
```
```
SELL:APPLE
ACK:APPLE
TRADE:APPLE
```

## Implementation Details

- Uses `tokio` for asynchronous networking.
- Utilizes `broadcast` channels to notify all clients of trades.
- Logs client connections, disconnections, and order activity.

## License

This project is released under the MIT License.

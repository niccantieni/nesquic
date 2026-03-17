# Implementation Design: Neqo IUT

This document describes how to implement the Neqo QUIC library as an IUT in nesquic. It is written against [REQUIREMENTS.md](REQUIREMENTS.md), which defines the trait contract all IUTs must satisfy. Read that document first.

The scaffolding already exists in `iut/neqo/` with `todo!()` stubs. This document specifies exactly what replaces each stub.

---

## 1. Overview

Neqo (`neqo-transport`) is Mozilla's QUIC implementation written in Rust. Unlike Quinn, which provides a high-level `async` API, Neqo exposes a **synchronous, poll-based state machine**. The caller is responsible for:

- Feeding incoming UDP datagrams into the connection via `process_input()`
- Draining outgoing datagrams via `process_output()`, sending each one over the socket
- Polling the event queue via `next_event()` after each processing step
- Sleeping until the next scheduled callback (from `Output::Callback(duration)`) or until the socket is readable

Our implementation wraps this synchronous state machine inside Tokio's async runtime using `tokio::select!` on socket readability and a sleep timer.

### Key differences from Quinn

| Concern | Quinn | Neqo |
|---------|-------|------|
| API style | Fully `async` (futures) | Synchronous state machine |
| TLS library | rustls (ring backend) | Mozilla NSS |
| Certificate loading | PEM files via rustls | NSS database nickname |
| Client cert validation | Validated against root CA | Explicit `authenticated()` call in event handler |
| Server connection management | `Endpoint::accept()` | `neqo_transport::server::Server` manages all connections |
| Event delivery | `await` on stream/connection | `next_event()` polling loop |
| Multiple connections | One `Endpoint`, one `Connection` | `Server::active_connections()` → `HashSet<ConnectionRef>` |

---

## 2. Dependencies (`iut/neqo/Cargo.toml`)

Current file has only `anyhow`, `neqo-transport`, `tokio`, and `utils`. The following must be added:

```toml
[dependencies]
anyhow        = "1.0.102"
socket2       = "0.6"
tokio         = { version = "1.50.0", features = ["macros", "full"] }
tracing       = "0.1"
utils         = { path = "../../utils" }

neqo-transport = { git = "https://github.com/mozilla/neqo.git", package = "neqo-transport", tag = "v0.23.1" }
neqo-crypto    = { git = "https://github.com/mozilla/neqo.git", package = "neqo-crypto",    tag = "v0.23.1" }
neqo-common    = { git = "https://github.com/mozilla/neqo.git", package = "neqo-common",    tag = "v0.23.1" }
```

**Why each addition:**
- `socket2` — low-level UDP socket creation with dual-stack IPv6 support (same as Quinn IUT)
- `tracing` — structured logging consistent with other IUTs
- `neqo-crypto` — provides `init()`, `init_db()`, `AntiReplay`, `AllowZeroRtt`, `AuthenticationStatus`
- `neqo-common` — provides `Datagram` (the byte-buffer type neqo uses for UDP payloads) and `event::Provider` (the `has_events()` / `next_event()` trait)

---

## 3. Library Code (`iut/neqo/src/lib.rs`)

The current `lib.rs` only re-exports `Client` and `Server`. It needs two additions:

### `bind_socket`

Identical purpose to Quinn's `bind_socket`: creates and binds a raw UDP socket using `socket2` for dual-stack IPv6 support, returns a `std::net::UdpSocket`.

```
fn bind_socket(addr: SocketAddr) -> Result<std::net::UdpSocket>
  1. Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))
  2. If IPv6: socket.set_only_v6(false)   // enable dual-stack
  3. socket.bind(SockAddr::from(addr))
  4. Return socket.into()   // converts socket2::Socket → std::net::UdpSocket
```

The result is handed to `tokio::net::UdpSocket::from_std()` by client and server.

### `init_crypto`

```
fn init_crypto() -> Result<()>
  neqo_crypto::init().context("failed to initialize NSS crypto")
```

Must be called exactly once before creating any `Connection` or `Server`. Both `Client::new` and `Server::new` call this. Calling it multiple times is safe (it is idempotent).

---

## 4. Certificate Loading

This is the most significant difference from Quinn. Neqo uses **Mozilla NSS** for TLS; it does not read PEM files directly. The `neqo_transport::server::Server::new()` `certs` parameter takes NSS **certificate nicknames** — string identifiers of certificates stored in an NSS database.

### Approach

Use `neqo_crypto::init_db(db_path)` instead of plain `init()` for the server. The `db_path` is a path to a directory containing an NSS database (`cert9.db`, `key4.db`, etc.) that was pre-populated with the server certificate and key under a known nickname (e.g., `"nesquic"`).

For the client: cert validation is bypassed by responding to the `AuthenticationNeeded` event with `conn.authenticated(AuthenticationStatus::Ok, now)`. The `args.cert` field from `ClientArgs` is not used for loading a trust anchor — the client unconditionally trusts the server certificate.

For the server:
- `args.cert` is treated as the path to the NSS database directory (not a PEM file path, despite the field name)
- The certificate nickname passed to `Server::new` is fixed as `"nesquic"` (the name under which the cert is stored in that NSS database)
- `args.key` is ignored (the NSS database contains both cert and key)

> **Note:** This means the `rcgen` tool or a setup script must also populate an NSS database at `res/nss/` containing the same certificate it writes to `res/pem/`, using `"nesquic"` as the nickname. This is documented as a prerequisite.

---

## 5. Client (`iut/neqo/src/client.rs`)

### Struct

```rust
pub struct Client {
    args: ClientArgs,
    conn: Option<neqo_transport::Connection>,
    socket: Option<tokio::net::UdpSocket>,
    local_addr: Option<SocketAddr>,
}
```

`conn` and `socket` are `None` until `connect()` is called.

### `new(args: ClientArgs) -> Result<Self>`

1. Call `init_crypto()`.
2. Return `Client { args, conn: None, socket: None, local_addr: None }`.

No connection is created yet — all connection setup is deferred to `connect()`.

### `connect(&mut self) -> Result<()>`

1. Resolve the remote address: parse `args.url` for hostname and port (default 4433), call `.to_socket_addrs()?.next()`.
2. Create and bind a local UDP socket at `[::]:0` using `bind_socket`.
3. Convert to `tokio::net::UdpSocket::from_std(socket)`.
4. Record `local_addr = socket.local_addr()`.
5. Create `Rc<RefCell<dyn ConnectionIdGenerator>>` using `RandomConnectionIdGenerator::new(8)`.
6. Call `neqo_transport::Connection::new_client(hostname, &["perf"], cid_gen, local_addr, remote_addr, ConnectionParameters::default(), Instant::now())`.
7. Store the connection in `self.conn`.
8. Run the **event loop** (see §7) until `ConnectionEvent::StateChange(State::Connected | State::Confirmed)` is observed.

### `run(&mut self) -> Result<()>`

Requires `connect()` to have been called.

1. Call `conn.stream_create(StreamType::BiDi)` to get a `stream_id`.
   - If `Error::StreamLimit` is returned, retry after the next `SendStreamCreatable(BiDi)` event.
2. Parse `args.blob` into a `perf::Request` (e.g., `"50Mbit"` → 6,250,000 bytes).
3. Call `conn.stream_send(stream_id, &request.to_bytes())` — sends the 8-byte header.
4. Call `conn.stream_close_send(stream_id)` — closes the send side (signals end of request).
5. Initialize `received_bytes: usize = 0` and a `read_buf: Vec<u8>` of size 32 KiB.
6. Run the **event loop** (see §7), handling:
   - `RecvStreamReadable { stream_id }`: call `conn.stream_recv(stream_id, &mut read_buf)` in a loop until `sz == 0` or `fin == true`. Accumulate `received_bytes += sz`. If `fin`, set `done = true`.
7. After the event loop exits (done): validate `received_bytes == request.size`. Return error if mismatch.
8. Close the connection gracefully: `conn.close(Instant::now(), 0, "done")`. Drain final output packets.

---

## 6. Server (`iut/neqo/src/server.rs`)

### Struct

```rust
pub struct Server {
    args: ServerArgs,
}
```

The `neqo_transport::server::Server` is constructed inside `listen()`, not in `new()`, because it requires `Instant::now()` at construction and owns non-`Send` types (`Rc<RefCell<...>>`).

### `new(args: ServerArgs) -> Result<Self>`

1. Call `init_crypto()` (or `init_db` if using an NSS database — see §4).
2. Return `Server { args }`.

### `listen(&mut self) -> Result<()>`

This is the main server loop.

**Setup:**

1. Bind a socket to `args.listen` via `bind_socket`. Convert to `tokio::net::UdpSocket::from_std(socket)`.
2. Create `AntiReplay::new(Instant::now(), Duration::from_secs(10), 7, 14)`.
3. Create `cid_gen = Rc::new(RefCell::new(RandomConnectionIdGenerator::new(8)))`.
4. Call `neqo_transport::server::Server::new(Instant::now(), &["nesquic"], &["perf"], anti_replay, Box::new(AllowZeroRtt), cid_gen, ConnectionParameters::default())`.
5. Initialize per-stream state maps:
   - `read_state: HashMap<StreamId, Vec<u8>>` — accumulates request bytes per stream
   - `write_state: HashMap<StreamId, WriteState>` — tracks response send progress

   Where `WriteState` holds `remaining: usize` (bytes still to send) and `offset: usize` (position in the zero-buffer).

**Main loop:**

```
loop {
    // 1. Process events on all active connections
    for conn_ref in server.active_connections() {
        while let Some(event) = conn_ref.borrow_mut().next_event() {
            handle_event(event, &conn_ref, &mut read_state, &mut write_state, now);
        }
    }

    // 2. Drive output
    loop {
        match server.process_output(now) {
            Output::Datagram(d) => socket.send_to(d.as_ref(), d.destination()).await?,
            Output::Callback(duration) => { set timeout = duration; break; }
            Output::None => break,
        }
    }

    // 3. Wait for input or timeout
    tokio::select! {
        _ = socket.readable() => {
            let (data, src) = socket.recv_from(&mut recv_buf).await?;
            let dgram = Datagram::new(src, local_addr, Tos::default(), data);
            server.process_input(std::iter::once(dgram), now);
        }
        _ = sleep(timeout) => { /* just re-loop */ }
    }
}
```

**Event handling (`handle_event`):**

```
NewStream { stream_id }
  → Insert stream_id into read_state (empty Vec) and write_state (pending, no data yet)

RecvStreamReadable { stream_id }
  → call conn.stream_recv(stream_id, &mut read_buf) in a loop until sz == 0 or fin
  → accumulate bytes into read_state[stream_id]
  → if we have >= 8 bytes and haven't parsed yet:
      parse first 8 bytes as Blob (big-endian size)
      store write_state[stream_id] = WriteState { remaining: blob.size, offset: 0 }
  → if fin: trigger send if not already started (move stream to writable)

SendStreamWritable { stream_id }
  → call send_response(stream_id, &conn_ref, &mut write_state)

StateChange(State::Connected)
  → (optional) send session ticket: conn.send_ticket(now, b"")
```

**Response sending (`send_response`):**

Uses a fixed 32 KiB zero-filled buffer (like neqo-reference's `SendData::zeroes`):

```
const ZERO_BUF: &[u8] = &[0u8; 32 * 1024];

while write_state.remaining > 0 {
    let chunk = &ZERO_BUF[..min(ZERO_BUF.len(), write_state.remaining)];
    match conn.stream_send(stream_id, chunk) {
        Ok(0) => return,   // flow-controlled, wait for SendStreamWritable
        Ok(n) => { write_state.remaining -= n; write_state.offset = (write_state.offset + n) % ZERO_BUF.len(); }
        Err(_) => { write_state.remove(stream_id); return; }
    }
}
// All data sent
conn.stream_close_send(stream_id)?;
write_state.remove(stream_id);
```

---

## 7. Event Loop Design

The core challenge: neqo's `Connection` is synchronous, but `bin::Client::connect()` and `bin::Client::run()` are `async fn`. The event loop wraps the synchronous state machine in a Tokio-compatible async loop.

### Shared pattern

Both `connect()` and `run()` use a helper:

```
async fn drive_until<F>(
    conn: &mut Connection,
    socket: &tokio::net::UdpSocket,
    local_addr: SocketAddr,
    mut done: F,              // closure: returns true when we can stop
) -> Result<()>
where
    F: FnMut(&mut Connection) -> Result<bool>
{
    let mut timeout: Option<Duration> = None;

    loop {
        // 1. Drain events, check done condition
        if done(conn)? {
            return Ok(());
        }

        // 2. Send all outgoing datagrams
        loop {
            match conn.process_output(Instant::now()) {
                Output::Datagram(d) => {
                    socket.send_to(d.as_ref(), d.destination()).await?;
                }
                Output::Callback(dur) => {
                    timeout = Some(dur);
                    break;
                }
                Output::None => {
                    timeout = None;
                    break;
                }
            }
        }

        // 3. If there are still pending events, loop again without waiting
        if conn.has_events() {
            continue;
        }

        // 4. Wait for socket data or timer
        let sleep_fut = async {
            match timeout {
                Some(dur) => tokio::time::sleep(dur).await,
                None => std::future::pending().await,
            }
        };

        tokio::select! {
            biased;
            result = socket.readable() => {
                result?;
                let mut recv_buf = vec![0u8; 65536];
                match socket.try_recv_from(&mut recv_buf) {
                    Ok((n, src)) => {
                        recv_buf.truncate(n);
                        let dgram = Datagram::new(src, local_addr, recv_buf);
                        conn.process_input(dgram, Instant::now());
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
            }
            _ = sleep_fut => {
                // timeout fired — just loop again to call process_output
            }
        }
    }
}
```

### `connect()` done condition

```rust
let mut connected = false;
drive_until(&mut conn, &socket, local_addr, |conn| {
    while let Some(event) = conn.next_event() {
        match event {
            ConnectionEvent::AuthenticationNeeded => {
                conn.authenticated(AuthenticationStatus::Ok, Instant::now());
            }
            ConnectionEvent::StateChange(State::Connected | State::Confirmed) => {
                connected = true;
            }
            ConnectionEvent::StateChange(State::Closed(err)) => {
                bail!("connection closed during handshake: {:?}", err);
            }
            _ => {}
        }
    }
    Ok(connected)
}).await
```

### `run()` done condition

```rust
let mut received = 0usize;
let request_size = request.size;
drive_until(&mut conn, &socket, local_addr, |conn| {
    while let Some(event) = conn.next_event() {
        match event {
            ConnectionEvent::RecvStreamReadable { stream_id } => {
                loop {
                    let (n, fin) = conn.stream_recv(stream_id, &mut read_buf)?;
                    received += n;
                    if fin || n == 0 { break; }
                }
            }
            ConnectionEvent::SendStreamCreatable { stream_type: StreamType::BiDi } => {
                // retry stream creation if it was flow-controlled earlier
                if stream_id.is_none() {
                    stream_id = Some(conn.stream_create(StreamType::BiDi)?);
                    // re-send request ...
                }
            }
            _ => {}
        }
    }
    Ok(received >= request_size)
}).await
```

---

## 8. Protocol Mapping

The `perf` protocol (from `utils/src/perf.rs`) maps to Neqo streams as follows:

### Client → Server (request)

| Step | Quinn | Neqo |
|------|-------|------|
| Open stream | `conn.open_bi().await` | `conn.stream_create(StreamType::BiDi)` — on `StateChange(Connected)` or `SendStreamCreatable(BiDi)` |
| Send 8 bytes | `send.write_all(&bytes).await` | `conn.stream_send(stream_id, &bytes)` — check returned `n == 8` |
| Close send side | `send.finish()` | `conn.stream_close_send(stream_id)` |

### Server → Client (response)

| Step | Quinn | Neqo |
|------|-------|------|
| Detect new stream | `conn.accept_bi().await` | `ConnectionEvent::NewStream { stream_id }` |
| Read request | `recv.read_to_end(64*1024)` | `conn.stream_recv(stream_id, buf)` loop on `RecvStreamReadable` until `fin == true` |
| Parse Blob | `Blob::try_from(req)` | Same — first 8 bytes of accumulated buffer |
| Write zeros | `send.write_chunk(Bytes::from_iter(blob))` | `conn.stream_send(stream_id, &ZERO_BUF[..chunk])` loop on `SendStreamWritable` |
| Close send side | `send.finish()` | `conn.stream_close_send(stream_id)` |

### Stream flow control

Neqo returns `Ok(0)` from `stream_send` when the send buffer is full (flow-controlled). When this happens the server must stop sending and wait for the next `SendStreamWritable` event before resuming. This is analogous to the `SendData` pattern in the neqo reference implementation.

---

## 9. ALPN Protocol

Set `"perf"` as the ALPN protocol:

- **Client**: pass `&["perf"]` as the `protocols` argument to `Connection::new_client`.
- **Server**: pass `&["perf"]` as the `protocols` argument to `neqo_transport::server::Server::new`.

This matches the Quinn IUT's `client_crypto.alpn_protocols = vec![b"perf".to_vec()]`.

---

## 10. Existing Scaffolding

The files `iut/neqo/src/lib.rs`, `client.rs`, and `server.rs` already have:
- Correct imports from `utils::{bin, bin::ClientArgs/ServerArgs}`
- Correct trait `impl` blocks (`impl bin::Client for Client`, `impl bin::Server for Server`)
- The `TARGET` constant for tracing
- `todo!()` stubs in all method bodies

No structural changes are needed — only the `todo!()` bodies must be filled in according to this document.

---

## 11. Non-Send Constraint

`Rc<RefCell<...>>` is `!Send`. The neqo `Connection` and `server::Server` types contain `Rc` internally and are therefore `!Send`. This means:

- The entire client event loop (`connect()` + `run()`) must execute on a single thread.
- Use `#[tokio::main(flavor = "current_thread")]` in the binary entry point, or spawn the neqo work with `tokio::task::LocalSet`.
- The server loop likewise runs on a single thread (the `listen()` method does not spawn tasks across threads).

This is a fundamental difference from Quinn, where each connection can be handled on any thread.

---

## 12. Prerequisites

Before running the Neqo IUT, the following must be in place:

1. **NSS database** at a known path (e.g., `res/nss/`) containing the server certificate and key under the nickname `"nesquic"`. This database is created once using the `certutil` tool from the NSS package:
   ```
   certutil -N -d res/nss/ --empty-password
   openssl pkcs12 -export -in res/pem/cert.pem -inkey res/pem/key.pem -out /tmp/nesquic.p12 -passout pass:
   pk12util -i /tmp/nesquic.p12 -d res/nss/ -W ""
   certutil -d res/nss/ -L   # verify "nesquic" nickname appears
   ```

2. **`ServerArgs.cert`** in the Neqo IUT is interpreted as the NSS database directory path (e.g., `res/nss/`), not a PEM file path. This is a behavioral difference from the Quinn IUT that must be documented in the server CLI help text.

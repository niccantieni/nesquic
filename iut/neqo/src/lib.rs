mod client;
mod server;

pub use client::Client;
pub use server::Server;

use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;

/// Create and bind a UDP socket. Enables dual-stack IPv6 when the address is IPv6.
/// Returns a standard std::net::UdpSocket ready to be handed to tokio.
pub(crate) fn bind_socket(addr: SocketAddr) -> Result<std::net::UdpSocket> {
    let socket = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))
        .context("create socket")?;

    if addr.is_ipv6() {
        socket.set_only_v6(false).context("set_only_v6")?;
    }

    socket
        .bind(&socket2::SockAddr::from(addr))
        .context("binding socket")?;

    Ok(socket.into())
}

/// Initialize NSS crypto without a certificate database (for clients).
pub(crate) fn init_crypto() -> Result<()> {
    neqo_crypto::init().context("failed to initialize NSS crypto")
}

/// Initialize NSS crypto with a certificate database (for servers).
pub(crate) fn init_crypto_db(db_path: &str) -> Result<()> {
    neqo_crypto::init_db(std::path::Path::new(db_path))
        .context("failed to initialize NSS crypto database")
}

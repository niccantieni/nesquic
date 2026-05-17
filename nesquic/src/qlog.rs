use anyhow::{bail, Result};
use glob::glob;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tracing::debug;

#[cfg(feature = "neqo")]
use neqo_iut::qlog_neqo as qlog_iut;
#[cfg(feature = "quinn")]
use quinn_iut::qlog_quinn as qlog_iut;
#[cfg(feature = "noq")]
use noq_iut::qlog_noq as qlog_iut;
#[cfg(feature = "quiche")]
use quiche_iut::qlog_quiche as qlog_iut;

#[derive(Debug, Clone, Copy)]
struct QlogPacket {
    packet_number: u64,
    time: f32,
}

#[derive(Debug, Clone, Copy)]
struct QlogResults {
    client_first_initial_pkt_sent: Option<QlogPacket>,
    client_last_handshake_pkt_sent: Option<QlogPacket>,
    server_last_handshake_pkt_received: Option<QlogPacket>,
    server_first_1rtt_pkt_sent: Option<QlogPacket>,
    client_first_1rtt_pkt_received: Option<QlogPacket>,
}

fn find_sqlog_file(dir: &Path, role: &str) -> Result<PathBuf> {
    let pattern = format!("{}/{}/*qlog", dir.display(), role);
    let file = glob(&pattern)?
        .next()
        .ok_or_else(|| anyhow::anyhow!("no {} sqlog file in {}", role, dir.display()))??;
    if !file.is_file() {
        bail!("expected a file at {}, got something else", file.display());
    }
    Ok(file)
}

fn open_qlog_file(path: &Path) -> Result<qlog_iut::reader::QlogSeqReader<'_>> {
    let reader = qlog_iut::reader::QlogSeqReader::new(Box::new(BufReader::new(
        File::open(path)?,
    )))
    .map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(reader)
}

/// Three-pass algorithm: extracts handshake latency from client + server qlog files.
///
/// Latency = time(client receives first 1-RTT from server) − time(client sends first Initial).
fn parse_handshake_latency(client_path: &Path, server_path: &Path) -> Result<f32> {
    let mut results = QlogResults {
        client_first_initial_pkt_sent: None,
        client_last_handshake_pkt_sent: None,
        server_last_handshake_pkt_received: None,
        server_first_1rtt_pkt_sent: None,
        client_first_1rtt_pkt_received: None,
    };

    // Pass 1: client — find first Initial sent, last Handshake sent
    let mut cq = open_qlog_file(client_path)?;
    let mut client_last_ts: f32 = 0.0;
    while let Some(event) = cq.next() {
        let qlog_iut::reader::Event::Qlog(ref event) = event else {
            bail!("unexpected non-qlog event in client file");
        };
        match event.data {
            qlog_iut::events::EventData::PacketSent(ref pkt) => {
                match pkt.header.packet_type {
                    qlog_iut::events::quic::PacketType::Initial => {
                        if event.time < client_last_ts {
                            bail!("client qlog events are out of order");
                        }
                        if results.client_first_initial_pkt_sent.is_none() {
                            results.client_first_initial_pkt_sent = Some(QlogPacket {
                                packet_number: pkt.header.packet_number.unwrap(),
                                time: event.time,
                            });
                        }
                    }
                    qlog_iut::events::quic::PacketType::Handshake => {
                        if event.time < client_last_ts {
                            bail!("client qlog events are out of order");
                        }
                        results.client_last_handshake_pkt_sent = Some(QlogPacket {
                            packet_number: pkt.header.packet_number.unwrap(),
                            time: event.time,
                        });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        client_last_ts = event.time;
    }

    let last_hs = results
        .client_last_handshake_pkt_sent
        .ok_or_else(|| anyhow::anyhow!("could not find client_last_handshake_pkt_sent"))?;

    // Pass 2: server — find the matching Handshake received, then first 1-RTT sent
    let mut sq = open_qlog_file(server_path)?;
    let mut server_last_ts: f32 = 0.0;
    while let Some(event) = sq.next() {
        let qlog_iut::reader::Event::Qlog(ref event) = event else {
            bail!("unexpected non-qlog event in server file");
        };
        match event.data {
            qlog_iut::events::EventData::PacketReceived(ref pkt) => {
                if matches!(pkt.header.packet_type, qlog_iut::events::quic::PacketType::Handshake) {
                    if event.time < server_last_ts {
                        bail!("server qlog events are out of order");
                    }
                    if pkt.header.packet_number.unwrap() == last_hs.packet_number {
                        results.server_last_handshake_pkt_received = Some(QlogPacket {
                            packet_number: pkt.header.packet_number.unwrap(),
                            time: event.time,
                        });
                    }
                }
            }
            qlog_iut::events::EventData::PacketSent(ref pkt) => {
                if matches!(pkt.header.packet_type, qlog_iut::events::quic::PacketType::OneRtt) {
                    if event.time < server_last_ts {
                        bail!("server qlog events are out of order");
                    }
                    if results.server_last_handshake_pkt_received.is_some()
                        && results.server_first_1rtt_pkt_sent.is_none()
                    {
                        results.server_first_1rtt_pkt_sent = Some(QlogPacket {
                            packet_number: pkt.header.packet_number.unwrap(),
                            time: event.time,
                        });
                    }
                }
            }
            _ => {}
        }
        server_last_ts = event.time;
        if results.server_first_1rtt_pkt_sent.is_some()
            && results.server_last_handshake_pkt_received.is_some()
        {
            break;
        }
    }

    let first_1rtt_srv = results
        .server_first_1rtt_pkt_sent
        .ok_or_else(|| anyhow::anyhow!("could not find server_first_1rtt_pkt_sent"))?;
    results
        .server_last_handshake_pkt_received
        .ok_or_else(|| anyhow::anyhow!("could not find server_last_handshake_pkt_received"))?;

    // Pass 3: client — find the matching 1-RTT received from server
    let mut cq = open_qlog_file(client_path)?;
    while let Some(event) = cq.next() {
        let qlog_iut::reader::Event::Qlog(ref event) = event else {
            bail!("unexpected non-qlog event in client file (pass 3)");
        };
        if let qlog_iut::events::EventData::PacketReceived(ref pkt) = event.data {
            if matches!(pkt.header.packet_type, qlog_iut::events::quic::PacketType::OneRtt) {
                if pkt.header.packet_number.unwrap() == first_1rtt_srv.packet_number {
                    results.client_first_1rtt_pkt_received = Some(QlogPacket {
                        packet_number: pkt.header.packet_number.unwrap(),
                        time: event.time,
                    });
                    break;
                }
            }
        }
    }

    let start = results
        .client_first_initial_pkt_sent
        .ok_or_else(|| anyhow::anyhow!("could not find client_first_initial_pkt_sent"))?;
    let end = results
        .client_first_1rtt_pkt_received
        .ok_or_else(|| anyhow::anyhow!("could not find client_first_1rtt_pkt_received"))?;

    let latency_ms = end.time - start.time;
    debug!("qlog handshake latency: {:.3} ms", latency_ms);
    Ok(latency_ms)
}

/// Parse qlog files from `qlog_dir/{client,server}/*qlog` and return an InfluxDB
/// line-protocol string for the `nesquic_qlog` measurement.
pub(crate) fn extract_metrics(
    qlog_dir: &Path,
    tag_str: &str,
    timestamp_ns: u128,
) -> Result<String> {
    let client_path = find_sqlog_file(qlog_dir, "client")?;
    let server_path = find_sqlog_file(qlog_dir, "server")?;
    let latency_ms = parse_handshake_latency(&client_path, &server_path)?;
    Ok(format!(
        "nesquic_qlog{} handshake_latency_ms={} {}",
        tag_str, latency_ms, timestamp_ns
    ))
}

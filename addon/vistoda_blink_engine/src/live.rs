use std::time::Duration;

use rustls::pki_types::ServerName;
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use url::Url;

use crate::{blink_client::BlinkClient, framing::ImmiDecoder, hub::PublisherGuard, tls::connector};

const BATTERY_DEADLINE: Duration = Duration::from_secs(75);
const POWERED_DEADLINE: Duration = Duration::from_secs(600);
const LATENCY_PACKET: [u8; 33] = [
    0x12, 0, 0, 3, 0xe8, 0, 0, 0, 0x18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0,
];

#[derive(Debug, Error)]
pub enum LiveError {
    #[error("Blink live negotiation failed")]
    Negotiation,
    #[error("Blink IMMI address is invalid")]
    Address,
    #[error("Blink IMMI transport failed: {0}")]
    Transport(#[from] std::io::Error),
    #[error("Blink IMMI TLS failed: {0}")]
    Tls(#[from] rustls::Error),
    #[error("Blink IMMI framing failed: {0}")]
    Framing(#[from] crate::framing::FramingError),
}

pub async fn produce(client: BlinkClient, alias: String, publisher: PublisherGuard) {
    if let Err(error) = produce_inner(&client, &alias, &publisher).await {
        publisher.record_protocol_error();
        tracing::warn!(%error, camera = %alias, "Blink live session ended");
    }
}

async fn produce_inner(
    client: &BlinkClient,
    alias: &str,
    publisher: &PublisherGuard,
) -> Result<(), LiveError> {
    let (camera, descriptor) = client
        .start_live(alias)
        .await
        .map_err(|_| LiveError::Negotiation)?;
    let deadline = if camera.powered {
        POWERED_DEADLINE
    } else {
        BATTERY_DEADLINE
    };
    let poll_client = client.clone();
    let poll_camera = camera.clone();
    let command_id = descriptor.command_id;
    let polling_interval = Duration::from_secs_f64(descriptor.polling_interval.max(0.25));
    let poller = tokio::spawn(async move {
        loop {
            tokio::time::sleep(polling_interval).await;
            if !poll_client
                .live_active(&poll_camera, command_id)
                .await
                .unwrap_or(false)
            {
                break;
            }
        }
    });
    let result = tokio::time::timeout(deadline, receive(&descriptor.server, publisher)).await;
    poller.abort();
    client.finish_live(&camera, descriptor.command_id).await;
    match result {
        Ok(value) => value,
        Err(_) => Ok(()),
    }
}

async fn receive(server: &str, publisher: &PublisherGuard) -> Result<(), LiveError> {
    let target = Target::parse(server)?;
    let tcp = TcpStream::connect((target.host.as_str(), target.port)).await?;
    let name = ServerName::try_from(target.host.clone()).map_err(|_| LiveError::Address)?;
    let mut tls = connector().connect(name, tcp).await?;
    tls.write_all(&auth_header(target.client_id, &target.connection_id))
        .await?;
    let (mut reader, mut writer) = tokio::io::split(tls);
    let keepalive = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        let mut sequence = 0_u32;
        let mut seconds = 0_u32;
        loop {
            tick.tick().await;
            if seconds.is_multiple_of(10) {
                sequence = sequence.wrapping_add(1);
                let mut packet = [0_u8; 9];
                packet[0] = 0x0a;
                packet[1..5].copy_from_slice(&sequence.to_be_bytes());
                writer.write_all(&packet).await?;
            }
            writer.write_all(&LATENCY_PACKET).await?;
            writer.flush().await?;
            seconds = seconds.wrapping_add(1);
        }
        #[allow(unreachable_code)]
        Ok::<(), std::io::Error>(())
    });
    let mut decoder = ImmiDecoder::default();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        for frame in decoder.push(&buffer[..read])? {
            publisher.publish(frame);
        }
        if !publisher.has_subscribers() {
            break;
        }
    }
    keepalive.abort();
    decoder.finish()?;
    Ok(())
}

pub(crate) struct Target {
    pub host: String,
    pub port: u16,
    pub client_id: u32,
    pub connection_id: String,
}

impl Target {
    pub(crate) fn parse(value: &str) -> Result<Self, LiveError> {
        let url = Url::parse(value).map_err(|_| LiveError::Address)?;
        if url.scheme() != "immis" {
            return Err(LiveError::Address);
        }
        let host = url.host_str().ok_or(LiveError::Address)?.to_owned();
        let port = url.port().ok_or(LiveError::Address)?;
        let client_id = url
            .query_pairs()
            .find(|(key, _)| key == "client_id")
            .and_then(|(_, value)| value.parse().ok())
            .ok_or(LiveError::Address)?;
        let connection_id = url
            .path_segments()
            .and_then(Iterator::last)
            .and_then(|value| value.split("__").next())
            .filter(|value| !value.is_empty())
            .ok_or(LiveError::Address)?
            .to_owned();
        Ok(Self {
            host,
            port,
            client_id,
            connection_id,
        })
    }
}

pub(crate) fn auth_header(client_id: u32, connection_id: &str) -> Vec<u8> {
    let mut value = Vec::with_capacity(122);
    value.extend([0, 0, 0, 0x28]);
    reserved_field(&mut value, 16);
    value.extend(client_id.to_be_bytes());
    value.extend([0x01, 0x08]);
    reserved_field(&mut value, 64);
    string_field(&mut value, connection_id, 16);
    value.extend([0, 0, 0, 1]);
    value
}

fn reserved_field(target: &mut Vec<u8>, width: usize) {
    target.extend(0_u32.to_be_bytes());
    target.resize(target.len() + width, 0);
}

fn string_field(target: &mut Vec<u8>, source: &str, width: usize) {
    target.extend(u32::try_from(width).unwrap_or(u32::MAX).to_be_bytes());
    let bytes = source.as_bytes();
    let length = bytes.len().min(width);
    target.extend(&bytes[..length]);
    target.resize(target.len() + width - length, 0);
}

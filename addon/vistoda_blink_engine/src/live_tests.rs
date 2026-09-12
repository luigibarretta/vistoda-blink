use crate::live::{Target, auth_header};
use crate::{hub::CameraHub, live::receive_stream};
use std::{error::Error, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};

#[test]
fn auth_packet_is_exact_and_address_is_bounded() {
    let Ok(target) = Target::parse("immis://example.invalid:443/session__x?client_id=7") else {
        panic!("valid fixture was rejected");
    };
    let packet = auth_header("SERIAL", target.client_id, &target.connection_id);
    assert_eq!(packet.len(), 122);
    assert_eq!(&packet[..4], &[0, 0, 0, 0x28]);
    assert_eq!(&packet[4..8], &16_u32.to_be_bytes());
    assert_eq!(&packet[8..14], b"SERIAL");
    assert_eq!(&packet[24..28], &7_u32.to_be_bytes());
    assert_eq!(&packet[30..34], &64_u32.to_be_bytes());
    assert_eq!(&packet[98..102], &16_u32.to_be_bytes());
    assert!(Target::parse("https://example.invalid:443/x?client_id=7").is_err());
}

#[tokio::test]
async fn cancelling_receiver_closes_the_writer_too() -> Result<(), Box<dyn Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let _subscriber = hub.subscribe();
    let (client, mut server) = tokio::io::duplex(1024);
    assert!(
        timeout(
            Duration::from_millis(20),
            receive_stream(client, &publisher)
        )
        .await
        .is_err()
    );
    let mut byte = [0];
    assert_eq!(
        timeout(Duration::from_millis(100), server.read(&mut byte)).await??,
        0
    );
    Ok(())
}

#[tokio::test]
async fn invalid_frame_closes_transport_without_a_detached_keepalive() -> Result<(), Box<dyn Error>>
{
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let _subscriber = hub.subscribe();
    let (client, mut server) = tokio::io::duplex(1024);
    server
        .write_all(&[0, 0, 0, 0, 0, 255, 255, 255, 255])
        .await?;
    assert!(receive_stream(client, &publisher).await.is_err());
    let mut byte = [0];
    assert_eq!(
        timeout(Duration::from_millis(100), server.read(&mut byte)).await??,
        0
    );
    Ok(())
}

#[tokio::test]
async fn an_idle_socket_closes_when_its_last_subscriber_leaves() -> Result<(), Box<dyn Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let subscriber = hub.subscribe();
    let (client, mut server) = tokio::io::duplex(1024);
    drop(subscriber);
    assert!(
        !timeout(
            Duration::from_millis(1500),
            receive_stream(client, &publisher)
        )
        .await??
    );
    let mut byte = [0];
    assert_eq!(server.read(&mut byte).await?, 0);
    Ok(())
}

#[tokio::test]
async fn audio_offer_does_not_replace_or_enable_any_media() -> Result<(), Box<dyn Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let mut subscriber = hub.subscribe();
    let (client, mut server) = tokio::io::duplex(1024);
    server.write_all(&[0x0c, 0xa0, 0, 0, 3, 0, 0, 0, 0]).await?;
    server
        .write_all(&[0, 0, 0, 0, 0, 0, 0, 0, 2, 0x47, 42])
        .await?;
    server.shutdown().await?;
    assert!(receive_stream(client, &publisher).await?);
    let crate::hub::HubMessage::Data(bytes) = subscriber.recv().await? else {
        panic!("expected unchanged media");
    };
    assert_eq!(bytes.as_ref(), &[0x47, 42]);
    let mut byte = [0];
    assert_eq!(server.read(&mut byte).await?, 0);
    Ok(())
}

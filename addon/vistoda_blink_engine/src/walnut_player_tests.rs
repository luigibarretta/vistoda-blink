use super::*;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn feeder_drains_when_both_process_pipes_would_block_one_serial_loop() -> io::Result<()> {
    let (input, mut process_input) = tokio::io::duplex(64);
    let (mut process_output, mut output) = tokio::io::duplex(64);
    let feeder = Feeder::spawn(input, MAX_DELAY);
    let expected = vec![0x47; 128 * 1024];
    feeder.push(Bytes::copy_from_slice(&expected))?;
    // A real muxer can stop reading stdin until its stdout is drained.
    let process = async {
        let mut block = [0; 128];
        loop {
            let count = process_input.read(&mut block).await?;
            if count == 0 {
                return Ok::<_, io::Error>(());
            }
            process_output.write_all(&block[..count]).await?;
        }
    };
    let consume = async {
        let mut actual = vec![0; expected.len()];
        output.read_exact(&mut actual).await?;
        assert_eq!(actual, expected);
        drop(feeder);
        Ok::<_, io::Error>(())
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::try_join!(process, consume)
    })
    .await
    .map_err(io::Error::other)??;
    Ok(())
}

#[tokio::test]
async fn dropping_feeder_cancels_blocked_write_and_closes_its_pipe() -> io::Result<()> {
    let (input, mut reader) = tokio::io::duplex(1);
    let feeder = Feeder::spawn(input, MAX_DELAY);
    feeder.push(Bytes::from_static(&[1, 2, 3]))?;
    tokio::task::yield_now().await;
    drop(feeder);
    let mut received = Vec::new();
    tokio::time::timeout(
        Duration::from_millis(100),
        reader.read_to_end(&mut received),
    )
    .await
    .map_err(io::Error::other)??;
    assert_eq!(received, [1]);
    Ok(())
}

#[tokio::test]
async fn queue_budgets_include_in_flight_bytes_and_reject_oversized_chunks() -> io::Result<()> {
    let (input, _reader) = tokio::io::duplex(1);
    let feeder = Feeder::spawn(input, MAX_DELAY);
    assert!(feeder.push(Bytes::new()).is_err());
    assert!(
        feeder
            .push(Bytes::from(vec![0; MAX_PACKET_BYTES + 1]))
            .is_err()
    );
    feeder.push(Bytes::from(vec![0; MAX_PACKET_BYTES]))?;
    tokio::task::yield_now().await;
    assert!(feeder.push(Bytes::from_static(&[1])).is_err());
    let (input, _reader) = tokio::io::duplex(1);
    let feeder = Feeder::spawn(input, MAX_DELAY);
    for _ in 0..QUEUE_CHUNKS {
        feeder.push(Bytes::from_static(&[1]))?;
    }
    assert!(feeder.push(Bytes::from_static(&[1])).is_err());
    Ok(())
}

#[tokio::test]
async fn stalled_write_reports_failure_without_waiting_for_more_input() -> io::Result<()> {
    let (input, _reader) = tokio::io::duplex(1);
    let mut feeder = Feeder::spawn(input, Duration::from_millis(20));
    feeder.push(Bytes::from_static(&[1, 2]))?;
    let result = tokio::time::timeout(Duration::from_millis(200), feeder.finished())
        .await
        .map_err(io::Error::other)?;
    assert!(result.is_err());
    assert_eq!(feeder.budget.available_permits(), MAX_PACKET_BYTES);
    Ok(())
}

#[tokio::test]
async fn expired_queued_chunk_is_never_written() -> io::Result<()> {
    let (input, mut reader) = tokio::io::duplex(8);
    let mut feeder = Feeder::spawn(input, MAX_DELAY);
    let permit = feeder
        .budget
        .clone()
        .try_acquire_owned()
        .map_err(io::Error::other)?;
    feeder
        .sender
        .try_send(Chunk {
            bytes: Bytes::from_static(&[0x47]),
            created: Instant::now()
                .checked_sub(MAX_DELAY)
                .ok_or_else(|| io::Error::other("test clock underflow"))?,
            _permit: permit,
        })
        .map_err(io::Error::other)?;
    assert!(feeder.finished().await.is_err());
    let mut received = Vec::new();
    reader.read_to_end(&mut received).await?;
    assert!(received.is_empty());
    assert_eq!(feeder.budget.available_permits(), MAX_PACKET_BYTES);
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn explicit_stop_joins_feeder_and_reaps_owned_process() -> io::Result<()> {
    use std::{path::Path, process::Stdio};
    let mut child = tokio::process::Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let process_id = child
        .id()
        .ok_or_else(|| io::Error::other("missing process id"))?;
    let input = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("missing stdin"))?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("missing stdout"))?;
    let player = WalnutPlayer {
        child,
        output,
        feeder: Feeder::spawn(input, MAX_DELAY),
    };
    player.feeder.push(Bytes::from(vec![0x47; 128 * 1024]))?;
    tokio::task::yield_now().await;
    player.stop().await;
    assert!(!Path::new(&format!("/proc/{process_id}")).exists());
    Ok(())
}

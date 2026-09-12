use super::*;

#[test]
fn drops_recover_without_reclaiming_but_actual_revocation_is_fatal() -> io::Result<()> {
    let mut stale = 0;
    let mut full = 0;
    handle_submit(Err(AudioLeaseError::Expired), &mut stale, &mut full)?;
    handle_submit(Err(AudioLeaseError::Backpressure), &mut stale, &mut full)?;
    handle_submit(Ok(()), &mut stale, &mut full)?;
    assert_eq!((stale, full), (1, 1));
    for error in [
        AudioLeaseError::Stale,
        AudioLeaseError::Closed,
        AudioLeaseError::Unavailable,
        AudioLeaseError::InvalidFrame,
    ] {
        assert!(handle_submit(Err(error), &mut stale, &mut full).is_err());
    }
    assert_eq!((stale, full), (1, 1));
    Ok(())
}

#[test]
fn priming_does_not_consume_pcm_and_partial_chunks_keep_their_age() -> io::Result<()> {
    let base = Instant::now();
    let mut provenance = Provenance::default();
    for offset in [0, 40, 80, 120] {
        provenance.record(640, base + Duration::from_millis(offset))?;
    }
    let now = base + Duration::from_millis(160);
    assert_eq!(provenance.take(now)?, None);
    assert_eq!(provenance.samples, 2560);
    assert_eq!(provenance.take(now)?, Some(base));
    assert_eq!(provenance.samples, 1536);
    assert_eq!(
        provenance.take(now)?,
        Some(base + Duration::from_millis(40))
    );
    assert_eq!(provenance.samples, 512);
    assert!(provenance.take(now).is_err());
    Ok(())
}

#[test]
fn delayed_encoder_output_cannot_become_fresh_again() -> io::Result<()> {
    let base = Instant::now();
    let mut provenance = Provenance::default();
    provenance.record(640, base)?;
    provenance.record(640, base + Duration::from_millis(40))?;
    assert_eq!(provenance.take(base)?, None);
    let delayed = base + MAX_AUDIO_AGE + Duration::from_nanos(1);
    let original = provenance.take(delayed)?;
    assert_eq!(original, Some(base));
    assert!(!fresh(base, delayed));
    assert_eq!(provenance.samples, 256);
    assert_eq!(
        provenance.pending.front().map(|(_, time)| *time),
        Some(base + Duration::from_millis(40))
    );
    Ok(())
}

#[test]
fn provenance_memory_and_freshness_boundary_are_bounded() -> io::Result<()> {
    let base = Instant::now();
    let mut provenance = Provenance::default();
    for _ in 0..6 {
        provenance.record(640, base)?;
    }
    assert!(provenance.record(640, base).is_err());
    assert_eq!(provenance.samples, 3840);
    assert_eq!(provenance.pending.len(), 6);
    assert_eq!(provenance.take(base)?, None);
    assert_eq!(provenance.take(base + MAX_AUDIO_AGE)?, Some(base));
    Ok(())
}

#[test]
fn expired_output_is_consumed_and_fresh_pcm_can_recover_without_redating() -> io::Result<()> {
    let base = Instant::now();
    let now = base + Duration::from_millis(300);
    let recent = base + Duration::from_millis(200);
    let mut provenance = Provenance::default();
    provenance.record(1024, base)?;
    provenance.record(1024, recent)?;
    assert_eq!(provenance.take(now)?, None); // Encoder priming only.
    assert_eq!(provenance.take(now)?, Some(base));
    assert!(!fresh(base, now)); // Discard encoded bytes, not the entire epoch.
    assert_eq!(provenance.take(now)?, Some(recent));
    assert!(fresh(recent, now));
    assert_eq!(provenance.samples, 0);
    assert!(provenance.pending.is_empty());
    assert!(!fresh(now + Duration::from_nanos(1), now));
    Ok(())
}

#[test]
fn microphone_errors_distinguish_contention_policy_and_encoder_failure() {
    for (error, expected) in [
        (AudioLeaseError::Busy, "busy"),
        (AudioLeaseError::Unavailable, "unavailable"),
        (AudioLeaseError::Closed, "disconnected"),
        (AudioLeaseError::Stale, "lease_revoked"),
        (AudioLeaseError::Expired, "audio_expired"),
    ] {
        assert_eq!(errors::reason(&io::Error::other(error)), expected);
    }
    assert_eq!(
        errors::reason(&io::Error::other(Failure::EncoderUnavailable)),
        "encoder_unavailable"
    );
    assert_eq!(
        errors::reason(&io::Error::other(Failure::PcmRate)),
        "pcm_rate"
    );
    assert_eq!(
        errors::reason(&io::Error::other(Failure::PcmBacklog)),
        "pcm_backlog"
    );
    assert_eq!(
        errors::reason(&io::Error::other(Failure::PcmWriteTimeout)),
        "pcm_write_timeout"
    );
}

#[tokio::test]
#[ignore = "Requires explicitly selected offline FFmpeg via VISTODA_AUDIO_CODEC_TEST_BIN"]
async fn real_aac_fixture_preserves_pcm_age_and_decodes() -> Result<(), Box<dyn std::error::Error>>
{
    use std::process::Stdio;
    use tokio::process::Command;
    let program = std::env::var("VISTODA_AUDIO_CODEC_TEST_BIN")?;
    // Match the deployed encoder flags; all media remains in process pipes.
    let arguments = "-hide_banner -loglevel error -nostdin -filter_threads 1 -filter_complex_threads 1 \
        -f s16le -ar 16000 -ac 1 -blocksize 1024 -probesize 32 -analyzeduration 0 -i pipe:0 \
        -map 0:a:0 -c:a aac -profile:a aac_low -b:a 32k -ar 16000 -ac 1 -threads 1 \
        -f adts -flush_packets 1 pipe:1";
    let mut child = Command::new(&program)
        .args(arguments.split_whitespace())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("stdin"))?;
    let mut output = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("stdout"))?;
    let mut decoder = AdtsDecoder::default();
    let mut provenance = Provenance::default();
    let mut tick = tokio::time::interval(Duration::from_millis(32));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let deadline = tokio::time::sleep(Duration::from_secs(2));
    tokio::pin!(deadline);
    let mut chunks = 0;
    let mut packets = 0;
    let mut encoded = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut ages = Vec::new();
    loop {
        tokio::select! {
            _ = tick.tick(), if chunks < 20 => {
                let pcm: Vec<u8> = (0..usize::from(PCM_SAMPLES)).flat_map(|sample| {
                    let value: i16 = if (chunks * usize::from(PCM_SAMPLES) + sample) % 36 < 18 { 800 } else { -800 };
                    value.to_le_bytes()
                }).collect();
                let created = Instant::now();
                input.write_all(&pcm).await?;
                provenance.record(usize::from(PCM_SAMPLES), created)?;
                chunks += 1;
            }
            count = output.read(&mut buffer) => {
                let count = count?;
                if count == 0 { return Err(io::Error::other("unexpected encoder EOF").into()); }
                for packet in decoder.push(&buffer[..count])? {
                    let now = Instant::now();
                    let oldest_age = provenance.pending.front().map(|(_, time)| now.duration_since(*time));
                    eprintln!("AAC output {packets}, oldest PCM age: {oldest_age:?}");
                    packets += 1;
                    if let Some(created) = provenance.take(now)? {
                        ages.push(now.duration_since(created));
                        encoded.extend_from_slice(&packet);
                    }
                }
                if chunks == 20 && ages.len() >= 8 { break; }
            }
            () = &mut deadline => return Err(io::Error::other("offline encoder timeout").into()),
        }
    }
    child.kill().await?;
    assert!(ages.len() >= 8);
    assert!(ages.iter().all(|age| *age <= MAX_AUDIO_AGE));
    eprintln!(
        "Verified {} non-priming AAC frames; maximum PCM age {:?}",
        ages.len(),
        ages.iter().max()
    );
    let mut decode = Command::new(program)
        .args("-hide_banner -loglevel error -f aac -i pipe:0 -map 0:a:0 -c:a pcm_s16le -f s16le pipe:1".split_whitespace())
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .kill_on_drop(true).spawn()?;
    let mut decode_input = decode
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("decode stdin"))?;
    decode_input.write_all(&encoded).await?;
    drop(decode_input);
    let pcm_result =
        tokio::time::timeout(Duration::from_secs(3), decode.wait_with_output()).await??;
    assert!(pcm_result.status.success());
    assert!(pcm_result.stdout.len() >= ages.len() * 2048);
    assert!(
        pcm_result
            .stdout
            .chunks_exact(2)
            .any(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]).unsigned_abs() > 100)
    );
    Ok(())
}

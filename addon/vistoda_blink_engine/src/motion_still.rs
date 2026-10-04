//! HA-local motion still: one JPEG from the motion recording's live stream,
//! served when Blink publishes no event thumbnail (ADR 0015).
//!
//! The capture only joins a live session that the motion recorder already
//! owns; it never starts a session or wakes a camera by itself.

use std::{
    collections::{HashMap, VecDeque},
    process::Stdio,
    time::{Duration, Instant},
};

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::{RwLock, Semaphore},
};

use crate::{
    hub::{EngineState, HubMessage, Subscriber},
    motion::MotionTracker,
};

const PROGRAM: &str = "/usr/local/bin/ffmpeg";
/// Decode keyframes only, scale into 1280×720 at full range and emit one JPEG.
const ARGUMENTS: &[&str] = &[
    "-hide_banner",
    "-loglevel",
    "error",
    "-nostdin",
    "-probesize",
    "1000000",
    "-analyzeduration",
    "2000000",
    "-skip_frame",
    "nokey",
    "-threads",
    "1",
    "-f",
    "mpegts",
    "-i",
    "pipe:0",
    "-map",
    "0:v:0",
    "-frames:v",
    "1",
    "-vf",
    "scale=w=1280:h=720:force_original_aspect_ratio=decrease:out_range=full,format=yuv420p",
    "-color_range",
    "pc",
    "-c:v",
    "mjpeg",
    "-q:v",
    "5",
    "-threads",
    "1",
    "-filter_threads",
    "1",
    "-f",
    "mjpeg",
    "pipe:1",
];
const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_JPEG_BYTES: usize = 1024 * 1024;
const MIN_JPEG_BYTES: usize = 128;
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);
/// The recorder acquires its live publisher right after it starts.
const PUBLISHER_WAIT: Duration = Duration::from_secs(10);
const PUBLISHER_POLL: Duration = Duration::from_millis(250);
const STILL_LIMIT: usize = 16;
/// At most two decoders run at once; extra events skip their still.
static ENCODERS: Semaphore = Semaphore::const_new(2);

/// Latest local stills by Blink event ID, bounded in count and size.
#[derive(Default)]
pub struct StillStore {
    inner: RwLock<(HashMap<String, Bytes>, VecDeque<String>)>,
}

impl MotionTracker {
    pub async fn local_still(&self, event_id: &str) -> Option<Bytes> {
        self.stills.inner.read().await.0.get(event_id).cloned()
    }

    pub(crate) async fn store_still(&self, event_id: &str, jpeg: Bytes) {
        {
            let mut guard = self.stills.inner.write().await;
            let (stills, order) = &mut *guard;
            if stills.insert(event_id.to_owned(), jpeg).is_none() {
                order.push_back(event_id.to_owned());
            }
            while order.len() > STILL_LIMIT {
                if let Some(old) = order.pop_front() {
                    stills.remove(&old);
                }
            }
        }
        // `/v1/motion` long-polls see `thumbnail_available` flip at once.
        self.mark_changed();
    }
}

/// Capture a still for `event_id` from the camera's running motion recording.
pub fn spawn(engine: EngineState, alias: String, event_id: String) {
    tokio::spawn(async move {
        if engine.motion().local_still(&event_id).await.is_some() {
            return;
        }
        let Ok(_permit) = ENCODERS.try_acquire() else {
            return;
        };
        let Some(subscriber) = join_live(&engine, &alias).await else {
            return;
        };
        let capture = extract(PROGRAM, ARGUMENTS, frames(subscriber));
        if let Ok(Some(jpeg)) = tokio::time::timeout(CAPTURE_TIMEOUT, capture).await {
            engine.motion().store_still(&event_id, jpeg).await;
            tracing::info!(camera = %alias, "local motion still captured");
        } else {
            tracing::info!(camera = %alias, "local motion still unavailable");
        }
    });
}

/// Subscribe only to a live session that already has a publisher.
async fn join_live(engine: &EngineState, alias: &str) -> Option<Subscriber> {
    let deadline = Instant::now() + PUBLISHER_WAIT;
    loop {
        let hub = engine.hubs.read().await.get(alias).cloned();
        if let Some(hub) = hub
            && hub.snapshot().publisher
        {
            return Some(hub.subscribe());
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(PUBLISHER_POLL).await;
    }
}

fn frames(mut subscriber: Subscriber) -> impl Stream<Item = Bytes> {
    async_stream::stream! {
        loop {
            match subscriber.recv().await {
                Ok(HubMessage::Data(frame)) => yield frame,
                Ok(HubMessage::End) | Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            }
        }
    }
}

/// Pipe MPEG-TS into the decoder until it prints one JPEG, the input cap is
/// reached or the stream ends. Pipes only: no file or network path.
pub async fn extract(
    program: &str,
    arguments: &[&str],
    input: impl Stream<Item = Bytes>,
) -> Option<Bytes> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let reader = async move {
        let mut jpeg = Vec::new();
        let limit = u64::try_from(MAX_JPEG_BYTES + 1).unwrap_or(u64::MAX);
        stdout.take(limit).read_to_end(&mut jpeg).await.ok()?;
        Some(jpeg)
    };
    let writer = async move {
        futures_util::pin_mut!(input);
        let mut fed = 0_usize;
        while let Some(chunk) = input.next().await {
            fed = fed.saturating_add(chunk.len());
            // Only MPEG-TS from the IMMI publisher; anything else ends the input.
            if chunk.first() != Some(&0x47)
                || fed > MAX_INPUT_BYTES
                || stdin.write_all(&chunk).await.is_err()
            {
                break;
            }
        }
        // Dropping stdin sends EOF so the decoder can flush a pending frame.
    };
    tokio::pin!(reader);
    let jpeg = tokio::select! {
        jpeg = &mut reader => jpeg,
        () = writer => reader.await,
    };
    let _ = child.start_kill();
    let _ = child.wait().await;
    jpeg.filter(|jpeg| is_jpeg(jpeg)).map(Bytes::from)
}

pub fn is_jpeg(value: &[u8]) -> bool {
    (MIN_JPEG_BYTES..=MAX_JPEG_BYTES).contains(&value.len())
        && value.starts_with(&[0xFF, 0xD8, 0xFF])
        && value.ends_with(&[0xFF, 0xD9])
}

#[cfg(test)]
#[path = "motion_still_tests.rs"]
mod tests;

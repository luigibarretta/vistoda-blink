//! Owned player feeder: stdin backpressure must never prevent draining stdout.
//!
//! Guard contract: eight chunks, 4 MiB INCLUDING the in-flight write, and one
//! second from enqueue through completed write. Overflow, age or pipe errors
//! close the live session; no TS bytes are silently dropped or retried.
//! Drop aborts the feeder and the child retains `kill_on_drop`; explicit stop
//! also joins/reaps. Offline duplex tests reproduce the two-full-pipes cycle.
use crate::{audio_codec::AudioCodec, framing::MAX_PACKET_BYTES};
use bytes::Bytes;
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    process::{Child, ChildStdout},
    sync::{OwnedSemaphorePermit, Semaphore, mpsc},
    task::JoinHandle,
};

const QUEUE_CHUNKS: usize = 8;
const MAX_DELAY: Duration = Duration::from_secs(1);

pub struct WalnutPlayer {
    child: Child,
    pub output: ChildStdout,
    pub feeder: Feeder,
}
impl WalnutPlayer {
    pub fn start() -> io::Result<Self> {
        let (child, input, output) = AudioCodec::player()?.into_parts();
        Ok(Self {
            child,
            output,
            feeder: Feeder::spawn(input, MAX_DELAY),
        })
    }
    pub async fn stop(mut self) {
        self.feeder.task.abort();
        let _ = self.child.start_kill();
        let _ = (&mut self.feeder.task).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
    }
}

struct Chunk {
    bytes: Bytes,
    created: Instant,
    _permit: OwnedSemaphorePermit,
}
pub struct Feeder {
    sender: mpsc::Sender<Chunk>,
    budget: Arc<Semaphore>,
    task: JoinHandle<io::Result<()>>,
}
impl Feeder {
    fn spawn<W: AsyncWrite + Unpin + Send + 'static>(mut input: W, max_delay: Duration) -> Self {
        let (sender, mut receiver) = mpsc::channel::<Chunk>(QUEUE_CHUNKS);
        let budget = Arc::new(Semaphore::new(MAX_PACKET_BYTES));
        let task = tokio::spawn(async move {
            while let Some(chunk) = receiver.recv().await {
                let remaining = max_delay
                    .checked_sub(chunk.created.elapsed())
                    .filter(|duration| !duration.is_zero())
                    .ok_or_else(|| io::Error::other("live remux input expired"))?;
                // The permit remains owned until the entire write completes.
                tokio::time::timeout(remaining, input.write_all(&chunk.bytes))
                    .await
                    .map_err(io::Error::other)??;
            }
            Ok(())
        });
        Self {
            sender,
            budget,
            task,
        }
    }
    pub fn push(&self, bytes: Bytes) -> io::Result<()> {
        if bytes.is_empty() || bytes.len() > MAX_PACKET_BYTES {
            return Err(io::Error::other("invalid live remux input size"));
        }
        let permit = self
            .budget
            .clone()
            .try_acquire_many_owned(u32::try_from(bytes.len()).map_err(io::Error::other)?)
            .map_err(io::Error::other)?;
        self.sender
            .try_send(Chunk {
                bytes,
                created: Instant::now(),
                _permit: permit,
            })
            .map_err(io::Error::other)
    }
    pub async fn finished(&mut self) -> io::Result<()> {
        (&mut self.task).await.map_err(io::Error::other)?
    }
}
impl Drop for Feeder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
#[path = "walnut_player_tests.rs"]
mod tests;

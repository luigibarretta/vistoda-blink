use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use bytes::Bytes;
use tokio::sync::{RwLock, broadcast};
use zeroize::Zeroizing;

use crate::{
    blink_client::{BlinkClient, BlinkError},
    credentials::CredentialStore,
    enrollment::EnrollmentManager,
    error::EngineError,
    live,
};

const QUEUE_DEPTH: usize = 12;

#[derive(Clone)]
pub enum HubMessage {
    Data(Bytes),
    End,
}

pub struct CameraHub {
    sender: broadcast::Sender<HubMessage>,
    publisher: AtomicBool,
    subscribers: AtomicUsize,
    packets: AtomicU64,
    lagged: AtomicU64,
    protocol_errors: AtomicU64,
}

impl CameraHub {
    fn new() -> Self {
        let (sender, _) = broadcast::channel(QUEUE_DEPTH);
        Self {
            sender,
            publisher: AtomicBool::new(false),
            subscribers: AtomicUsize::new(0),
            packets: AtomicU64::new(0),
            lagged: AtomicU64::new(0),
            protocol_errors: AtomicU64::new(0),
        }
    }

    pub fn acquire_publisher(self: &Arc<Self>) -> Result<PublisherGuard, EngineError> {
        self.publisher
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| EngineError::PublisherBusy)?;
        Ok(PublisherGuard { hub: self.clone() })
    }

    pub fn subscribe(self: &Arc<Self>) -> Subscriber {
        self.subscribers.fetch_add(1, Ordering::Relaxed);
        Subscriber {
            receiver: self.sender.subscribe(),
            hub: self.clone(),
        }
    }

    pub fn publish(&self, frame: Bytes) {
        self.packets.fetch_add(1, Ordering::Relaxed);
        let _ = self.sender.send(HubMessage::Data(frame));
    }

    pub fn record_protocol_error(&self) {
        self.protocol_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn has_subscribers(&self) -> bool {
        self.subscribers.load(Ordering::Relaxed) > 0
    }

    pub(crate) fn snapshot(&self) -> HubSnapshot {
        HubSnapshot {
            publisher: self.publisher.load(Ordering::Relaxed),
            subscribers: self.subscribers.load(Ordering::Relaxed),
            packets: self.packets.load(Ordering::Relaxed),
            lagged: self.lagged.load(Ordering::Relaxed),
            protocol_errors: self.protocol_errors.load(Ordering::Relaxed),
        }
    }
}

pub struct PublisherGuard {
    hub: Arc<CameraHub>,
}

impl PublisherGuard {
    pub fn publish(&self, frame: Bytes) {
        self.hub.publish(frame);
    }

    pub fn record_protocol_error(&self) {
        self.hub.record_protocol_error();
    }

    pub fn has_subscribers(&self) -> bool {
        self.hub.has_subscribers()
    }
}

impl Drop for PublisherGuard {
    fn drop(&mut self) {
        self.hub.publisher.store(false, Ordering::Release);
        let _ = self.hub.sender.send(HubMessage::End);
    }
}

pub struct Subscriber {
    receiver: broadcast::Receiver<HubMessage>,
    hub: Arc<CameraHub>,
}

impl Subscriber {
    pub async fn recv(&mut self) -> Result<HubMessage, broadcast::error::RecvError> {
        match self.receiver.recv().await {
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                self.hub.lagged.fetch_add(skipped, Ordering::Relaxed);
                Err(broadcast::error::RecvError::Lagged(skipped))
            }
            result => result,
        }
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.hub.subscribers.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub struct EngineState {
    token: Arc<Zeroizing<String>>,
    pub(crate) hubs: Arc<RwLock<HashMap<String, Arc<CameraHub>>>>,
    client: BlinkClient,
    enrollment: EnrollmentManager,
}

impl EngineState {
    pub fn new(token: Zeroizing<String>, credentials_path: PathBuf) -> Result<Self, BlinkError> {
        let client = BlinkClient::new(CredentialStore::new(credentials_path, &token))?;
        Ok(Self {
            token: Arc::new(token),
            hubs: Arc::new(RwLock::new(HashMap::new())),
            enrollment: EnrollmentManager::new(client.clone()),
            client,
        })
    }

    pub async fn initialize(&self) -> Result<bool, BlinkError> {
        self.client.bootstrap().await
    }

    pub fn token(&self) -> &str {
        self.token.as_str()
    }

    async fn hub(&self, alias: &str) -> Arc<CameraHub> {
        if let Some(hub) = self.hubs.read().await.get(alias).cloned() {
            return hub;
        }
        self.hubs
            .write()
            .await
            .entry(alias.to_owned())
            .or_insert_with(|| Arc::new(CameraHub::new()))
            .clone()
    }

    pub fn client(&self) -> &BlinkClient {
        &self.client
    }
    pub fn enrollment(&self) -> &EnrollmentManager {
        &self.enrollment
    }

    pub async fn subscribe(&self, alias: &str) -> Result<Subscriber, EngineError> {
        if !self
            .client
            .state()
            .await
            .cameras
            .iter()
            .any(|camera| camera.alias == alias)
        {
            return Err(EngineError::CameraNotFound);
        }
        let hub = self.hub(alias).await;
        let subscriber = hub.subscribe();
        if let Ok(publisher) = hub.acquire_publisher() {
            tokio::spawn(live::produce(
                self.client.clone(),
                alias.to_owned(),
                publisher,
            ));
        }
        Ok(subscriber)
    }
}
pub(crate) struct HubSnapshot {
    pub publisher: bool,
    pub subscribers: usize,
    pub packets: u64,
    pub lagged: u64,
    pub protocol_errors: u64,
}

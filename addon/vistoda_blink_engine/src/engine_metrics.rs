use serde::Serialize;

use crate::hub::{EngineState, HubSnapshot};

#[derive(Serialize)]
pub struct Health {
    pub status: &'static str,
    pub version: &'static str,
    pub cameras: usize,
    pub publishers: usize,
    pub subscribers: usize,
    pub enrolled: bool,
}

impl Health {
    fn from_snapshots(snapshots: &[HubSnapshot], enrolled: bool) -> Self {
        Self {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
            cameras: snapshots.len(),
            publishers: snapshots.iter().filter(|value| value.publisher).count(),
            subscribers: snapshots.iter().map(|value| value.subscribers).sum(),
            enrolled,
        }
    }
}

impl EngineState {
    pub async fn health(&self) -> Health {
        let snapshots = {
            let hubs = self.hubs.read().await;
            hubs.values().map(|hub| hub.snapshot()).collect::<Vec<_>>()
        };
        Health::from_snapshots(&snapshots, self.client().enrolled().await)
    }

    pub async fn metrics(&self) -> String {
        let values = {
            let hubs = self.hubs.read().await;
            hubs.values().map(|hub| hub.snapshot()).collect::<Vec<_>>()
        };
        let packets: u64 = values.iter().map(|value| value.packets).sum();
        let lagged: u64 = values.iter().map(|value| value.lagged).sum();
        let errors: u64 = values.iter().map(|value| value.protocol_errors).sum();
        format!(
            "# TYPE vistoda_blink_packets_total counter\n\
             vistoda_blink_packets_total {packets}\n\
             # TYPE vistoda_blink_lagged_packets_total counter\n\
             vistoda_blink_lagged_packets_total {lagged}\n\
             # TYPE vistoda_blink_protocol_errors_total counter\n\
             vistoda_blink_protocol_errors_total {errors}\n"
        )
    }
}

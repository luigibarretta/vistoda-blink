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
    fn from_snapshots(snapshots: &[HubSnapshot], enrolled: bool, cameras: usize) -> Self {
        Self {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
            cameras,
            publishers: snapshots.iter().filter(|value| value.publisher).count(),
            subscribers: snapshots.iter().map(|value| value.subscribers).sum(),
            enrolled,
        }
    }
}

impl EngineState {
    pub async fn health(&self) -> Health {
        let cameras = self.client().state().await.cameras.len();
        let snapshots = {
            let hubs = self.hubs.read().await;
            hubs.values().map(|hub| hub.snapshot()).collect::<Vec<_>>()
        };
        Health::from_snapshots(&snapshots, self.client().enrolled().await, cameras)
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

#[cfg(test)]
mod tests {
    use super::{Health, HubSnapshot};

    #[test]
    fn health_counts_provider_inventory_instead_of_active_stream_hubs() {
        let active_hub = HubSnapshot {
            publisher: true,
            subscribers: 2,
            packets: 10,
            lagged: 0,
            protocol_errors: 0,
        };

        let health = Health::from_snapshots(&[active_hub], true, 5);

        assert_eq!(health.cameras, 5);
        assert_eq!(health.publishers, 1);
        assert_eq!(health.subscribers, 2);
    }
}

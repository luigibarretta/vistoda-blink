use std::fmt::Write;

use serde::Serialize;

use crate::{
    blink_session_health::SessionHealthSnapshot,
    hub::{EngineState, HubSnapshot},
};

/// Every motion poller state, exported one-hot so dashboards see transitions.
const POLLER_STATES: [&str; 6] = [
    "starting",
    "active",
    "idle",
    "backoff",
    "unauthorized",
    "disabled",
];

#[derive(Serialize)]
pub struct Health {
    pub status: &'static str,
    pub version: &'static str,
    pub cameras: usize,
    pub publishers: usize,
    pub subscribers: usize,
    pub enrolled: bool,
    /// Blink rejected the refresh grant: a new sign-in is required.
    pub reauth_required: bool,
    /// False while a rotated refresh token is only in memory (write retrying).
    pub credentials_persisted: bool,
}

impl Health {
    fn from_snapshots(
        snapshots: &[HubSnapshot],
        enrolled: bool,
        cameras: usize,
        session: SessionHealthSnapshot,
    ) -> Self {
        Self {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
            cameras,
            publishers: snapshots.iter().filter(|value| value.publisher).count(),
            subscribers: snapshots.iter().map(|value| value.subscribers).sum(),
            enrolled,
            reauth_required: session.reauth_required,
            credentials_persisted: !session.persist_pending,
        }
    }
}

impl EngineState {
    pub async fn health(&self) -> Health {
        let cameras = self.client().state().await.cameras.len();
        let snapshots = self.hub_snapshots().await;
        Health::from_snapshots(
            &snapshots,
            self.client().enrolled().await,
            cameras,
            self.client().session_health(),
        )
    }

    async fn hub_snapshots(&self) -> Vec<HubSnapshot> {
        let hubs = self.hubs.read().await;
        hubs.values().map(|hub| hub.snapshot()).collect()
    }

    pub async fn metrics(&self) -> String {
        let values = self.hub_snapshots().await;
        let poller = self.motion().status().await;
        let mut out = String::new();
        let mut metric = |kind: &str, name: &str, labels: &str, value: u64| {
            let _ = writeln!(out, "# TYPE vistoda_blink_{name} {kind}");
            let _ = writeln!(out, "vistoda_blink_{name}{labels} {value}");
        };
        metric(
            "counter",
            "packets_total",
            "",
            values.iter().map(|v| v.packets).sum(),
        );
        metric(
            "counter",
            "lagged_packets_total",
            "",
            values.iter().map(|v| v.lagged).sum(),
        );
        let errors = values.iter().map(|value| value.protocol_errors).sum();
        metric("counter", "protocol_errors_total", "", errors);
        let active = values.iter().filter(|value| value.publisher).count();
        metric(
            "gauge",
            "live_sessions_active",
            "",
            u64::try_from(active).unwrap_or(u64::MAX),
        );
        let subscribers = values.iter().map(|value| value.subscribers).sum::<usize>();
        metric(
            "gauge",
            "live_subscribers",
            "",
            u64::try_from(subscribers).unwrap_or(u64::MAX),
        );
        metric(
            "counter",
            "motion_poll_failures_total",
            "",
            self.motion().poll_failures(),
        );
        let limited = poller.rate_limited_until.is_some();
        metric("gauge", "motion_rate_limited", "", u64::from(limited));
        let session = self.client().session_health();
        metric(
            "gauge",
            "reauth_required",
            "",
            u64::from(session.reauth_required),
        );
        metric(
            "gauge",
            "credentials_persist_pending",
            "",
            u64::from(session.persist_pending),
        );
        metric(
            "counter",
            "credentials_persist_failures_total",
            "",
            session.persist_failures,
        );
        let state = if poller.state.is_empty() {
            "starting"
        } else {
            poller.state
        };
        let _ = writeln!(out, "# TYPE vistoda_blink_motion_poller_state gauge");
        for name in POLLER_STATES {
            let value = u8::from(name == state);
            let _ = writeln!(
                out,
                "vistoda_blink_motion_poller_state{{state=\"{name}\"}} {value}"
            );
        }
        let _ = writeln!(out, "# TYPE vistoda_blink_oauth_refreshes_total counter");
        for (result, value) in [
            ("success", session.refresh_success),
            ("failure", session.refresh_failure),
            ("rejected", session.refresh_rejected),
        ] {
            let _ = writeln!(
                out,
                "vistoda_blink_oauth_refreshes_total{{result=\"{result}\"}} {value}"
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{Health, HubSnapshot, SessionHealthSnapshot};

    #[test]
    fn health_counts_provider_inventory_instead_of_active_stream_hubs() {
        let active_hub = HubSnapshot {
            publisher: true,
            subscribers: 2,
            packets: 10,
            lagged: 0,
            protocol_errors: 0,
        };

        let session = SessionHealthSnapshot {
            persist_pending: true,
            ..SessionHealthSnapshot::default()
        };
        let health = Health::from_snapshots(&[active_hub], true, 5, session);

        assert_eq!(health.cameras, 5);
        assert_eq!(health.publishers, 1);
        assert_eq!(health.subscribers, 2);
        assert!(!health.credentials_persisted && !health.reauth_required);
    }
}

//! Slow program reads during the state refresh and verified enable/disable writes.

use std::time::{Duration, Instant};

use reqwest::StatusCode;

use crate::{
    blink_client::{BlinkClient, BlinkError, RequestContext},
    blink_model::NetworkState,
    blink_programs::{
        PROGRAM_REFRESH, Program, parse_programs, program_toggle_path, programs_path, valid_id,
    },
};

const READ_BACK_ATTEMPTS: usize = 3;
const READ_BACK_DELAY: Duration = Duration::from_secs(1);

impl BlinkClient {
    /// Fresh programs for every network, or `None` while the cache is younger
    /// than [`PROGRAM_REFRESH`] so a five-minute HA refresh adds no Blink load.
    /// A failing network keeps its previous programs.
    pub(crate) async fn refreshed_programs(
        &self,
        context: &RequestContext,
        networks: &[NetworkState],
    ) -> Option<Vec<Program>> {
        {
            let mut read_at = self.inner.programs_read_at.lock().await;
            if read_at.is_some_and(|at| at.elapsed() < PROGRAM_REFRESH) {
                return None;
            }
            *read_at = Some(Instant::now());
        }
        let previous = self.inner.state.read().await.programs.clone();
        let mut programs = Vec::new();
        for network in networks.iter().filter(|network| valid_id(&network.id)) {
            match self.network_programs(context, &network.id).await {
                Ok(list) => programs.extend(list),
                // Networks without the program feature answer 404: no programs.
                Err(BlinkError::Transport(error))
                    if error.status() == Some(StatusCode::NOT_FOUND) => {}
                Err(error) => {
                    // The transport error embeds the URL with account and network IDs.
                    let status = match &error {
                        BlinkError::Transport(error) => error.status().map(|code| code.as_u16()),
                        _ => None,
                    };
                    tracing::warn!(?status, "Blink program list unavailable; keeping the cache");
                    programs.extend(
                        previous
                            .iter()
                            .filter(|program| program.network_id == network.id)
                            .cloned(),
                    );
                }
            }
        }
        Some(programs)
    }

    /// Enable or disable one existing program and confirm it by reading it back.
    pub async fn set_program_enabled(
        &self,
        network_id: &str,
        program_id: &str,
        enabled: bool,
    ) -> Result<Program, BlinkError> {
        if !valid_id(program_id) {
            return Err(BlinkError::ProgramNotFound);
        }
        if !self
            .state()
            .await
            .networks
            .iter()
            .any(|network| network.id == network_id && valid_id(&network.id))
        {
            return Err(BlinkError::NetworkNotFound);
        }
        // Provider writes are serialized with the camera settings writes.
        let _guard = self.inner.settings_lock.lock().await;
        let context = self.context().await?;
        let current = self.network_programs(&context, network_id).await?;
        let program = find(&current, program_id)?;
        self.store_programs(network_id, current).await;
        if program.enabled == enabled {
            return Ok(program);
        }
        let path = program_toggle_path(&context.account_id, network_id, program_id, enabled);
        self.post_ack(&context, &path).await?;
        for attempt in 0..READ_BACK_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(READ_BACK_DELAY).await;
            }
            let list = self.network_programs(&context, network_id).await?;
            let program = find(&list, program_id)?;
            self.store_programs(network_id, list).await;
            if program.enabled == enabled {
                return Ok(program);
            }
        }
        tracing::warn!("Blink program state differs from the requested state after the write");
        Err(BlinkError::SettingsVerification)
    }

    async fn network_programs(
        &self,
        context: &RequestContext,
        network_id: &str,
    ) -> Result<Vec<Program>, BlinkError> {
        let value = self
            .get_json(context, &programs_path(&context.account_id, network_id))
            .await?;
        parse_programs(&value, network_id).ok_or(BlinkError::InvalidResponse)
    }

    /// Replace one network's cached programs after a verified read.
    async fn store_programs(&self, network_id: &str, list: Vec<Program>) {
        let mut state = self.inner.state.write().await;
        state
            .programs
            .retain(|program| program.network_id != network_id);
        state.programs.extend(list);
    }
}

fn find(list: &[Program], program_id: &str) -> Result<Program, BlinkError> {
    list.iter()
        .find(|program| program.id == program_id)
        .cloned()
        .ok_or(BlinkError::ProgramNotFound)
}

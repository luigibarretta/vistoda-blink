use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    blink_client::{BlinkClient, BlinkError},
    credentials::ProviderCredentials,
    oauth::{self, PendingOAuth, SignInResult},
};

const ENROLLMENT_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_PENDING: usize = 4;

struct Pending {
    oauth: PendingOAuth,
    username: String,
    created: Instant,
}

#[derive(Clone)]
pub struct EnrollmentManager {
    client: BlinkClient,
    pending: Arc<Mutex<HashMap<Uuid, Pending>>>,
}

#[derive(Deserialize, Zeroize)]
#[zeroize(drop)]
pub struct StartRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Zeroize)]
#[zeroize(drop)]
pub struct CompleteRequest {
    pub code: String,
}

#[derive(Deserialize, Zeroize)]
#[zeroize(drop)]
pub struct ImportRequest {
    pub refresh_token: String,
    pub hardware_id: String,
    pub region_id: Option<String>,
    pub account_id: Option<String>,
    pub user_id: Option<String>,
    pub username: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EnrollmentOutcome {
    Enrolled,
    TwoFactorRequired { enrollment_id: Uuid },
}

impl EnrollmentManager {
    pub fn new(client: BlinkClient) -> Self {
        Self {
            client,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(&self, mut request: StartRequest) -> Result<EnrollmentOutcome, BlinkError> {
        let username = Zeroizing::new(std::mem::take(&mut request.username));
        let password = Zeroizing::new(std::mem::take(&mut request.password));
        match oauth::sign_in(&username, &password).await? {
            SignInResult::Complete(completion) => {
                self.client
                    .enroll(completion, Some(username.to_string()))
                    .await?;
                Ok(EnrollmentOutcome::Enrolled)
            }
            SignInResult::TwoFactor(oauth) => {
                let mut pending = self.pending.lock().await;
                expire(&mut pending);
                if pending.len() >= MAX_PENDING {
                    return Err(BlinkError::InvalidResponse);
                }
                let enrollment_id = Uuid::new_v4();
                pending.insert(
                    enrollment_id,
                    Pending {
                        oauth,
                        username: username.to_string(),
                        created: Instant::now(),
                    },
                );
                drop(pending);
                Ok(EnrollmentOutcome::TwoFactorRequired { enrollment_id })
            }
        }
    }

    pub async fn complete(
        &self,
        id: Uuid,
        request: CompleteRequest,
    ) -> Result<EnrollmentOutcome, BlinkError> {
        let pending = {
            self.pending
                .lock()
                .await
                .remove(&id)
                .ok_or(BlinkError::InvalidResponse)?
        };
        if pending.created.elapsed() > ENROLLMENT_TTL {
            return Err(BlinkError::InvalidResponse);
        }
        let mut request = request;
        let code = Zeroizing::new(std::mem::take(&mut request.code));
        let completion = pending.oauth.complete(&code).await?;
        self.client
            .enroll(completion, Some(pending.username))
            .await?;
        Ok(EnrollmentOutcome::Enrolled)
    }

    pub async fn import(
        &self,
        mut request: ImportRequest,
    ) -> Result<EnrollmentOutcome, BlinkError> {
        self.client
            .import(ProviderCredentials {
                refresh_token: std::mem::take(&mut request.refresh_token),
                hardware_id: std::mem::take(&mut request.hardware_id),
                region_id: request.region_id.take(),
                account_id: request.account_id.take(),
                user_id: request.user_id.take(),
                username: request.username.take(),
            })
            .await?;
        Ok(EnrollmentOutcome::Enrolled)
    }
}

fn expire(pending: &mut HashMap<Uuid, Pending>) {
    pending.retain(|_, value| value.created.elapsed() <= ENROLLMENT_TTL);
}

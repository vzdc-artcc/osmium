pub mod audience;
pub mod config;
pub mod outbox;
pub mod render;
pub mod rsx;
pub mod service;
pub mod ses;
pub mod suppression;
pub mod templates;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailWorkerHealth {
    pub enabled: bool,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_finished_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_result_ok: Option<bool>,
    pub pending_count: Option<i64>,
}

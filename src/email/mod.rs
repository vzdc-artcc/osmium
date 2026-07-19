pub mod audience;
pub mod branding;
pub mod config;
pub mod outbox;
pub mod render;
pub mod rsx;
pub mod service;
pub mod ses;
pub mod suppression;
pub mod templates;

pub type EmailWorkerHealth = crate::jobs::JobHealthRecord<EmailWorkerMetrics>;

#[derive(Debug, Clone, Default)]
pub struct EmailWorkerMetrics {
    pub pending_count: Option<i64>,
}

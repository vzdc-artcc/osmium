use std::sync::{Arc, RwLock};

use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::sync::broadcast;

use crate::{
    email::{EmailWorkerHealth, service::EmailService},
    jobs::{
        JobHealthRecord,
        appointments_sync::AppointmentsSyncMetrics,
        event_lifecycle::EventLifecycleMetrics,
        faa_preferred_routes::FaaPreferredRoutesMetrics,
        ip_log_cleanup::IpLogCleanupMetrics,
        ip_log_writer::IpLogWriterMetrics,
        loa_expiration::LoaExpirationMetrics,
        roster_sync::RosterSyncMetrics,
        solo_expiration::SoloExpirationMetrics,
        stats_sync::{ControllerLifecycleEvent, StatsSyncMetrics},
    },
    repos::ip_request_log::IpRequestLogEntry,
};
use tokio::sync::{Mutex, mpsc};

#[derive(Clone, Default)]
pub struct JobHealth {
    pub stats_sync: StatsSyncHealth,
    pub roster_sync: RosterSyncHealth,
    pub event_automation: EventAutomationHealth,
    pub loa_expiration: LoaExpirationHealth,
    pub solo_expiration: SoloExpirationHealth,
    pub appointments_sync: AppointmentsSyncHealth,
    pub faa_preferred_routes: FaaPreferredRoutesHealth,
    pub ip_log_writer: IpLogWriterHealth,
    pub ip_log_cleanup: IpLogCleanupHealth,
}

pub type EventAutomationHealth = JobHealthRecord<EventLifecycleMetrics>;
pub type LoaExpirationHealth = JobHealthRecord<LoaExpirationMetrics>;
pub type SoloExpirationHealth = JobHealthRecord<SoloExpirationMetrics>;
pub type AppointmentsSyncHealth = JobHealthRecord<AppointmentsSyncMetrics>;
pub type FaaPreferredRoutesHealth = JobHealthRecord<FaaPreferredRoutesMetrics>;
pub type IpLogWriterHealth = JobHealthRecord<IpLogWriterMetrics>;
pub type IpLogCleanupHealth = JobHealthRecord<IpLogCleanupMetrics>;

#[derive(Clone, Default)]
pub struct EmailHealth {
    pub worker: EmailWorkerHealth,
}

pub type RosterSyncHealth = JobHealthRecord<RosterSyncMetrics>;
pub type StatsSyncEnvironmentHealth = JobHealthRecord<StatsSyncMetrics>;

#[derive(Clone, Default)]
pub struct StatsSyncHealth {
    pub enabled: bool,
    pub live: StatsSyncEnvironmentHealth,
}

impl StatsSyncHealth {
    /// Statistics only sync the live feed, so every environment maps to it.
    pub fn environment_mut(&mut self, _environment: &str) -> &mut StatsSyncEnvironmentHealth {
        &mut self.live
    }

    pub fn environment(&self, _environment: &str) -> &StatsSyncEnvironmentHealth {
        &self.live
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Option<PgPool>,
    pub job_health: Arc<RwLock<JobHealth>>,
    pub email_health: Arc<RwLock<EmailHealth>>,
    pub email: Arc<EmailService>,
    pub controller_events: broadcast::Sender<ControllerLifecycleEvent>,
    /// Shared in-memory per-IP rate limiter (spec 010). `Arc` so every router
    /// layer clone throttles against the same buckets.
    pub rate_limiter: Arc<crate::rate_limit::IpRateLimiter>,
    /// Dedicated tight limiter for the expensive GDPR data export, keyed per user.
    /// Only enforced when `rate_limit_enabled`.
    pub data_export_limiter: Arc<crate::rate_limit::IpRateLimiter>,
    /// Even tighter limiter for the admin mass (whole-roster) data export — the
    /// single most expensive request in the API. Keyed by the admin's user id.
    pub mass_data_export_limiter: Arc<crate::rate_limit::IpRateLimiter>,
    /// Whether the limiter is enforced. Off in the test harness so unrelated
    /// tests don't trip it.
    pub rate_limit_enabled: bool,
    /// Sender half of the durable IP-request-log buffer (spec 011). The request
    /// middleware pushes one entry per request here; the drain job flushes them.
    pub ip_log_tx: mpsc::Sender<IpRequestLogEntry>,
    /// Receiver half, behind a mutex so the single drain job (and the DB tests'
    /// synchronous-flush hook) can take from it.
    pub ip_log_rx: Arc<Mutex<mpsc::Receiver<IpRequestLogEntry>>>,
    /// Whether per-request IP metadata is buffered. Off in the test harness.
    pub ip_log_enabled: bool,
}

impl AppState {
    pub async fn from_env() -> Result<Self, sqlx::Error> {
        let email = Arc::new(EmailService::from_env().await);
        let (controller_events, _) = broadcast::channel(1024);
        let rate_limiter = crate::rate_limit::build_rate_limiter();
        let data_export_limiter = crate::rate_limit::build_data_export_limiter();
        let mass_data_export_limiter = crate::rate_limit::build_mass_data_export_limiter();
        let rate_limit_enabled = crate::config::rate_limit_enabled();
        let (ip_log_tx, ip_log_rx) = build_ip_log_channel();
        let ip_log_enabled = crate::config::ip_request_log_enabled();
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .connect(&database_url)
                .await?;
            return Ok(Self {
                db: Some(pool),
                job_health: Arc::new(RwLock::new(JobHealth::default())),
                email_health: Arc::new(RwLock::new(EmailHealth::default())),
                email,
                controller_events,
                rate_limiter,
                data_export_limiter,
                mass_data_export_limiter,
                rate_limit_enabled,
                ip_log_tx,
                ip_log_rx,
                ip_log_enabled,
            });
        }

        Ok(Self {
            db: None,
            job_health: Arc::new(RwLock::new(JobHealth::default())),
            email_health: Arc::new(RwLock::new(EmailHealth::default())),
            email,
            controller_events,
            rate_limiter,
            data_export_limiter,
            mass_data_export_limiter,
            rate_limit_enabled,
            ip_log_tx,
            ip_log_rx,
            ip_log_enabled,
        })
    }

    pub fn without_db() -> Self {
        let email = Arc::new(EmailService::disabled());
        let (controller_events, _) = broadcast::channel(1024);
        let (ip_log_tx, ip_log_rx) = build_ip_log_channel();
        Self {
            db: None,
            job_health: Arc::new(RwLock::new(JobHealth::default())),
            email_health: Arc::new(RwLock::new(EmailHealth::default())),
            email,
            controller_events,
            rate_limiter: crate::rate_limit::build_rate_limiter(),
            data_export_limiter: crate::rate_limit::build_data_export_limiter(),
            mass_data_export_limiter: crate::rate_limit::build_mass_data_export_limiter(),
            // Off by default: this DB-less state is test-only, and rate limiting is
            // meaningless without a DB (the bypass check can't run). Keeps the
            // `without_db()`-based test suites from being throttled incidentally.
            rate_limit_enabled: false,
            ip_log_tx,
            ip_log_rx,
            // Off for the same reason — no DB to flush to, and it keeps the
            // DB-less test suites from producing log traffic.
            ip_log_enabled: false,
        }
    }
}

/// Builds the bounded IP-request-log channel from the configured capacity.
fn build_ip_log_channel() -> (
    mpsc::Sender<IpRequestLogEntry>,
    Arc<Mutex<mpsc::Receiver<IpRequestLogEntry>>>,
) {
    let (tx, rx) = mpsc::channel(crate::config::ip_request_log_channel_capacity());
    (tx, Arc::new(Mutex::new(rx)))
}

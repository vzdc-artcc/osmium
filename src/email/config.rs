#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub enabled: bool,
    pub region: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub reply_to_address: Option<String>,
    pub ses_endpoint: Option<String>,
    pub worker_enabled: bool,
    pub worker_interval_secs: u64,
    pub worker_batch_size: i64,
    pub unsubscribe_base_url: Option<String>,
    pub unsubscribe_secret: Option<String>,
}

impl EmailConfig {
    pub fn from_env() -> Self {
        let enabled = truthy_env("EMAIL_ENABLED").unwrap_or(true);
        let worker_enabled = truthy_env("EMAIL_WORKER_ENABLED").unwrap_or(true);
        let worker_interval_secs = std::env::var("EMAIL_WORKER_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5)
            .max(1);
        let worker_batch_size = std::env::var("EMAIL_WORKER_BATCH_SIZE")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(25)
            .max(1);

        Self {
            enabled,
            region: std::env::var("AWS_REGION")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            from_address: std::env::var("EMAIL_FROM_ADDRESS")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            from_name: std::env::var("EMAIL_FROM_NAME")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            reply_to_address: std::env::var("EMAIL_REPLY_TO_ADDRESS")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            ses_endpoint: std::env::var("EMAIL_SES_ENDPOINT")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            worker_enabled,
            worker_interval_secs,
            worker_batch_size,
            unsubscribe_base_url: std::env::var("EMAIL_UNSUBSCRIBE_BASE_URL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            unsubscribe_secret: std::env::var("EMAIL_UNSUBSCRIBE_SECRET")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| {
                    std::env::var("FILE_SIGNING_SECRET")
                        .ok()
                        .filter(|v| !v.trim().is_empty())
                }),
        }
    }

    pub fn transport_enabled(&self) -> bool {
        self.enabled
            && self.region.is_some()
            && self.from_address.is_some()
            && std::env::var("AWS_ACCESS_KEY_ID").is_ok()
            && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok()
    }
}

fn truthy_env(name: &str) -> Option<bool> {
    std::env::var(name).ok().map(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::EmailConfig;

    #[test]
    fn config_defaults_worker_settings() {
        let config = EmailConfig::from_env();
        assert!(config.worker_interval_secs >= 1);
        assert!(config.worker_batch_size >= 1);
    }
}

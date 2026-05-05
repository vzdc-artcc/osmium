use anyhow::{Result, bail};
use uuid::Uuid;

use crate::{state::AppState, target};

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub async fn record_warning(
    state: &mut AppState,
    domain: &str,
    entity_type: &str,
    source_id: &str,
    message: impl Into<String>,
) -> Result<()> {
    let message = message.into();
    state
        .report
        .warning(domain, entity_type, source_id.to_string(), message.clone());
    if !state.config.dry_run {
        target::record_warning(
            &state.target,
            &state.config.run_id,
            domain,
            entity_type,
            source_id,
            &message,
        )
        .await?;
    }
    if state.config.abort_on_warning {
        bail!("{domain}/{entity_type}/{source_id}: {message}");
    }
    Ok(())
}

pub fn merge_note(prefix: &str, note: Option<&str>) -> String {
    match note.map(str::trim).filter(|value| !value.is_empty()) {
        Some(note) => format!("{prefix} {note}"),
        None => prefix.to_string(),
    }
}

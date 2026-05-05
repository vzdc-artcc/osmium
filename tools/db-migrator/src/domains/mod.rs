pub mod events;
pub mod feedback;
pub mod org;
pub mod reference;
pub mod training;
pub mod users;

use anyhow::{Result, bail};

use crate::{config::Domain, state::AppState, target};

pub async fn run_migration(state: &mut AppState) -> Result<()> {
    for domain in state.config.domains.clone() {
        match domain {
            Domain::Reference => reference::migrate(state).await?,
            Domain::Users => users::migrate(state).await?,
            Domain::Org => org::migrate(state).await?,
            Domain::Training => training::migrate(state).await?,
            Domain::Feedback => feedback::migrate(state).await?,
            Domain::Events => events::migrate(state).await?,
            Domain::Web => bail!("web migration is intentionally not implemented in v1"),
        }

        if !state.config.dry_run {
            target::checkpoint(
                &state.target,
                &state.config.run_id,
                domain_name(domain),
                "domain-complete",
            )
            .await?;
        }
    }

    Ok(())
}

pub fn domain_name(domain: Domain) -> &'static str {
    match domain {
        Domain::Reference => "reference",
        Domain::Users => "users",
        Domain::Org => "org",
        Domain::Training => "training",
        Domain::Feedback => "feedback",
        Domain::Events => "events",
        Domain::Web => "web",
    }
}

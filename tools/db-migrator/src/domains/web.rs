use anyhow::Result;
use serde::Serialize;
use sqlx::FromRow;

use crate::{helpers::record_warning, state::AppState};

const DOMAIN: &str = "web";

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceWelcomeMessages {
    home_text: String,
    visitor_text: String,
}

pub async fn migrate(state: &mut AppState) -> Result<()> {
    migrate_welcome_messages(state).await?;
    Ok(())
}

/// Legacy `WelcomeMessages` (a single row of `homeText`/`visitorText`) maps onto
/// osmium's `web.site_settings` row keyed `welcome_messages`, whose value is a
/// `{"homeText":..,"visitorText":..}` jsonb blob (already seeded empty by
/// migration 0013). This is an update of that one row, not an insert.
async fn migrate_welcome_messages(state: &mut AppState) -> Result<()> {
    let source = sqlx::query_as::<_, SourceWelcomeMessages>(
        r#"
        select
            "homeText" as home_text,
            "visitorText" as visitor_text
        from public."WelcomeMessages"
        order by id asc
        limit 1
        "#,
    )
    .fetch_optional(&state.source)
    .await?;

    let Some(row) = source else {
        // No legacy welcome-message content; the seeded empty row stands.
        record_warning(
            state,
            DOMAIN,
            "welcome_messages",
            "welcome_messages",
            "no legacy WelcomeMessages row found; leaving seeded site_settings value untouched",
        )
        .await?;
        return Ok(());
    };

    state.report.domain_mut(DOMAIN).planned += 1;

    let value = serde_json::json!({
        "homeText": row.home_text,
        "visitorText": row.visitor_text,
    });

    if !state.config.dry_run {
        let updated = sqlx::query(
            r#"
            update web.site_settings
            set value = $1, updated_at = now()
            where key = 'welcome_messages'
            "#,
        )
        .bind(&value)
        .execute(&state.target)
        .await?;

        if updated.rows_affected() == 0 {
            // The seed migration (0013) should have created this row; if it is
            // missing, insert it rather than silently dropping the content.
            sqlx::query(
                r#"
                insert into web.site_settings (key, value)
                values ('welcome_messages', $1)
                on conflict (key) do update set value = excluded.value, updated_at = now()
                "#,
            )
            .bind(&value)
            .execute(&state.target)
            .await?;
        }
    }

    state.report.domain_mut(DOMAIN).updated += 1;
    Ok(())
}

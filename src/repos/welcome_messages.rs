use chrono::{DateTime, Utc};
use sqlx::{Executor, PgPool, Postgres};

use crate::{errors::ApiError, models::WelcomeMessageContent};

const WELCOME_MESSAGES_KEY: &str = "welcome_messages";

pub async fn fetch_welcome_message_content(
    pool: &PgPool,
) -> Result<WelcomeMessageContent, ApiError> {
    let value = sqlx::query_scalar::<_, serde_json::Value>(
        "select value from web.site_settings where key = $1",
    )
    .bind(WELCOME_MESSAGES_KEY)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let Some(value) = value else {
        return Ok(WelcomeMessageContent {
            home_text: String::new(),
            visitor_text: String::new(),
        });
    };

    Ok(WelcomeMessageContent {
        home_text: value
            .get("homeText")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        visitor_text: value
            .get("visitorText")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

pub async fn update_welcome_message_content<'e, E>(
    executor: E,
    home_text: &str,
    visitor_text: &str,
    updated_by_user_id: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let value = serde_json::json!({
        "homeText": home_text,
        "visitorText": visitor_text,
    });

    sqlx::query(
        r#"
        insert into web.site_settings (key, value, updated_by_user_id, updated_at)
        values ($1, $2, $3, $4)
        on conflict (key) do update
        set value = excluded.value,
            updated_by_user_id = excluded.updated_by_user_id,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(WELCOME_MESSAGES_KEY)
    .bind(value)
    .bind(updated_by_user_id)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub struct MyWelcomeStateRow {
    pub show_welcome_message: bool,
    pub controller_status: Option<String>,
}

pub async fn fetch_my_welcome_state(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<MyWelcomeStateRow>, ApiError> {
    sqlx::query_as::<_, MyWelcomeStateRow>(
        r#"
        select p.show_welcome_message, m.controller_status
        from identity.user_profiles p
        left join org.memberships m on m.user_id = p.user_id
        where p.user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

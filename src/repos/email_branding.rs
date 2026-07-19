use chrono::{DateTime, Utc};
use sqlx::{Executor, Postgres};

use crate::{errors::ApiError, models::EmailBranding, models::UpdateEmailBrandingRequest};

const BRANDING_ID: &str = "default";

const BRANDING_COLUMNS: &str = r#"
    brand_name,
    tagline,
    footer_text,
    logo_file_id,
    header_background_color,
    header_text_color,
    page_background_color,
    panel_background_color,
    text_color,
    heading_color,
    link_color,
    accent_color,
    button_background_color,
    button_text_color,
    heading_font_family,
    body_font_family,
    font_size_scale,
    corner_style,
    updated_at
"#;

pub async fn fetch_branding<'e, E>(executor: E) -> Result<Option<EmailBranding>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, EmailBranding>(&format!(
        "select {BRANDING_COLUMNS} from email.branding where id = $1"
    ))
    .bind(BRANDING_ID)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn logo_file_is_public<'e, E>(executor: E, file_id: &str) -> Result<bool, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, bool>("select is_public from media.file_assets where id = $1")
        .bind(file_id)
        .fetch_optional(executor)
        .await
        .map_err(|_| ApiError::Internal)
        .map(|value| value.unwrap_or(false))
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_branding<'e, E>(
    executor: E,
    input: &UpdateEmailBrandingRequest,
    updated_by_user_id: Option<&str>,
    now: DateTime<Utc>,
) -> Result<EmailBranding, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, EmailBranding>(&format!(
        r#"
        insert into email.branding (
            id,
            brand_name,
            tagline,
            footer_text,
            logo_file_id,
            header_background_color,
            header_text_color,
            page_background_color,
            panel_background_color,
            text_color,
            heading_color,
            link_color,
            accent_color,
            button_background_color,
            button_text_color,
            heading_font_family,
            body_font_family,
            font_size_scale,
            corner_style,
            updated_by_user_id,
            updated_at
        )
        values (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21
        )
        on conflict (id) do update
        set brand_name = excluded.brand_name,
            tagline = excluded.tagline,
            footer_text = excluded.footer_text,
            logo_file_id = excluded.logo_file_id,
            header_background_color = excluded.header_background_color,
            header_text_color = excluded.header_text_color,
            page_background_color = excluded.page_background_color,
            panel_background_color = excluded.panel_background_color,
            text_color = excluded.text_color,
            heading_color = excluded.heading_color,
            link_color = excluded.link_color,
            accent_color = excluded.accent_color,
            button_background_color = excluded.button_background_color,
            button_text_color = excluded.button_text_color,
            heading_font_family = excluded.heading_font_family,
            body_font_family = excluded.body_font_family,
            font_size_scale = excluded.font_size_scale,
            corner_style = excluded.corner_style,
            updated_by_user_id = excluded.updated_by_user_id,
            updated_at = excluded.updated_at
        returning {BRANDING_COLUMNS}
        "#
    ))
    .bind(BRANDING_ID)
    .bind(&input.brand_name)
    .bind(&input.tagline)
    .bind(&input.footer_text)
    .bind(&input.logo_file_id)
    .bind(&input.header_background_color)
    .bind(&input.header_text_color)
    .bind(&input.page_background_color)
    .bind(&input.panel_background_color)
    .bind(&input.text_color)
    .bind(&input.heading_color)
    .bind(&input.link_color)
    .bind(&input.accent_color)
    .bind(&input.button_background_color)
    .bind(&input.button_text_color)
    .bind(&input.heading_font_family)
    .bind(&input.body_font_family)
    .bind(&input.font_size_scale)
    .bind(&input.corner_style)
    .bind(updated_by_user_id)
    .bind(now)
    .fetch_one(executor)
    .await
    .map_err(super::map_constraint_error)
}

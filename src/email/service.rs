use std::{collections::BTreeMap, sync::Arc};

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

use crate::{
    auth::context::{CurrentServiceAccount, CurrentUser},
    errors::ApiError,
    models::{
        EmailAudienceRequest, EmailOutboxDetailResponse, EmailOutboxListItem,
        EmailOutboxRecipientResponse, EmailPreviewResponse, EmailRecipientsRequest,
        EmailResubscribeRequest, EmailSendRequest, EmailSendResponse,
        EmailSuppressionRecordResponse, EmailTemplateDefinitionResponse, EmailUnsubscribeResponse,
        ListEmailOutboxQuery,
    },
    repos::audit,
};

use super::{
    audience::{ResolvedRecipient, audience_to_value, normalize_email, resolve_audience},
    config::EmailConfig,
    outbox,
    render::render_template,
    ses::SesMailer,
    suppression::{
        create_suppression, is_suppressed, revoke_suppression, verify_unsubscribe_token,
    },
    templates::{find_template, registry},
};

#[derive(Clone)]
pub struct EmailService {
    pub config: EmailConfig,
    mailer: Arc<SesMailer>,
}

#[derive(Debug, Clone)]
pub struct EmailActor {
    pub actor_id: Option<String>,
    pub user_id: Option<String>,
    pub service_account_id: Option<String>,
    pub request_source: String,
}

impl EmailService {
    pub fn disabled() -> Self {
        let config = EmailConfig::from_env();
        let mailer = Arc::new(SesMailer::disabled(config.clone()));
        Self { config, mailer }
    }

    pub async fn from_env() -> Self {
        let config = EmailConfig::from_env();
        let mailer = Arc::new(SesMailer::from_config(config.clone()).await);
        Self { config, mailer }
    }

    pub fn is_available(&self) -> bool {
        self.config.transport_enabled() && self.config.enabled
    }

    pub fn worker_enabled(&self) -> bool {
        self.is_available() && self.config.worker_enabled
    }

    pub fn templates(&self) -> Vec<EmailTemplateDefinitionResponse> {
        registry()
            .iter()
            .map(|template| EmailTemplateDefinitionResponse {
                id: template.id.to_string(),
                name: template.name.to_string(),
                category: template.category.to_string(),
                is_transactional: template.is_transactional,
                description: template.description.to_string(),
                allow_arbitrary_addresses: template.allow_arbitrary_addresses,
                required_payload_schema: (template.payload_schema)(),
            })
            .collect()
    }

    pub fn preview_template(
        &self,
        template_id: &str,
        payload: &Value,
    ) -> Result<EmailPreviewResponse, ApiError> {
        self.ensure_available()?;
        let template = find_template(template_id).ok_or(ApiError::BadRequest)?;
        let rendered = render_template(
            template,
            payload,
            self.config.unsubscribe_base_url.as_deref(),
            self.config.unsubscribe_secret.as_deref(),
            None,
            None,
        )?;
        Ok(EmailPreviewResponse {
            template_id: template_id.to_string(),
            subject: rendered.subject,
            html: rendered.html,
            text: rendered.text,
        })
    }

    pub async fn enqueue_template_send(
        &self,
        pool: &PgPool,
        actor: EmailActor,
        request: EmailSendRequest,
    ) -> Result<EmailSendResponse, ApiError> {
        self.ensure_available()?;
        let template = find_template(&request.template_id).ok_or(ApiError::BadRequest)?;
        let recipient_mode = match (request.recipients.is_some(), request.audience.is_some()) {
            (true, true) => "mixed",
            (true, false) => "explicit",
            (false, true) => "audience",
            (false, false) => return Err(ApiError::BadRequest),
        };

        let resolved = self
            .resolve_recipients(
                pool,
                template.id,
                template.allow_arbitrary_addresses,
                request.recipients.as_ref(),
                request.audience.as_ref(),
            )
            .await?;

        let preview = render_template(
            template,
            &request.payload,
            self.config.unsubscribe_base_url.as_deref(),
            self.config.unsubscribe_secret.as_deref(),
            resolved
                .deliverable
                .first()
                .map(|recipient| recipient.email.as_str()),
            resolved
                .deliverable
                .first()
                .and_then(|recipient| recipient.user_id.as_deref()),
        )?;
        if preview.subject.trim().is_empty() {
            return Err(ApiError::BadRequest);
        }

        if request.dry_run.unwrap_or(false) {
            return Ok(EmailSendResponse {
                id: None,
                template_id: template.id.to_string(),
                status: "validated".to_string(),
                resolved_recipients: resolved.deliverable.len(),
                suppressed_recipients: resolved.suppressed.len(),
                queued_at: None,
            });
        }

        let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
        let (id, queued_at) = outbox::queue_email(
            &mut tx,
            outbox::QueueEmailInput {
                template_id: template.id.to_string(),
                category: template.category.to_string(),
                is_transactional: template.is_transactional,
                requested_by_actor_id: actor.actor_id.clone(),
                requested_by_user_id: actor.user_id.clone(),
                requested_by_service_account_id: actor.service_account_id.clone(),
                request_source: actor.request_source.clone(),
                subject_override: request.subject_override.clone(),
                reply_to_address: normalize_optional_email(request.reply_to_address.as_deref()),
                payload: request.payload.clone(),
                recipient_mode: recipient_mode.to_string(),
                audience_filter: audience_to_value(request.audience.as_ref()),
                recipients: resolved.deliverable.clone(),
                suppressed_recipients: resolved.suppressed.clone(),
            },
        )
        .await?;

        audit::record_audit(
            &mut *tx,
            audit::AuditEntryInput {
                actor_id: actor.actor_id,
                action: "QUEUE".to_string(),
                resource_type: "EMAIL".to_string(),
                resource_id: Some(id.to_string()),
                scope_type: "global".to_string(),
                scope_key: Some(template.id.to_string()),
                before_state: None,
                after_state: Some(audit::sanitize_value(serde_json::json!({
                    "template_id": template.id,
                    "category": template.category,
                    "recipient_count": resolved.deliverable.len(),
                    "suppressed_count": resolved.suppressed.len(),
                    "request_source": actor.request_source,
                }))),
                ip_address: None,
            },
        )
        .await?;

        tx.commit().await.map_err(|_| ApiError::Internal)?;

        Ok(EmailSendResponse {
            id: Some(id.to_string()),
            template_id: template.id.to_string(),
            status: "pending".to_string(),
            resolved_recipients: resolved.deliverable.len(),
            suppressed_recipients: resolved.suppressed.len(),
            queued_at: Some(queued_at),
        })
    }

    pub async fn enqueue_to_users(
        &self,
        pool: &PgPool,
        actor: EmailActor,
        template_id: String,
        payload: Value,
        user_ids: Vec<String>,
    ) -> Result<EmailSendResponse, ApiError> {
        self.enqueue_template_send(
            pool,
            actor,
            EmailSendRequest {
                template_id,
                payload,
                recipients: Some(EmailRecipientsRequest {
                    users: user_ids,
                    emails: Vec::new(),
                }),
                audience: None,
                subject_override: None,
                reply_to_address: None,
                dry_run: Some(false),
            },
        )
        .await
    }

    pub async fn enqueue_to_addresses(
        &self,
        pool: &PgPool,
        actor: EmailActor,
        template_id: String,
        payload: Value,
        emails: Vec<String>,
    ) -> Result<EmailSendResponse, ApiError> {
        self.enqueue_template_send(
            pool,
            actor,
            EmailSendRequest {
                template_id,
                payload,
                recipients: Some(EmailRecipientsRequest {
                    users: Vec::new(),
                    emails,
                }),
                audience: None,
                subject_override: None,
                reply_to_address: None,
                dry_run: Some(false),
            },
        )
        .await
    }

    pub async fn enqueue_audience_send(
        &self,
        pool: &PgPool,
        actor: EmailActor,
        template_id: String,
        payload: Value,
        audience: EmailAudienceRequest,
    ) -> Result<EmailSendResponse, ApiError> {
        self.enqueue_template_send(
            pool,
            actor,
            EmailSendRequest {
                template_id,
                payload,
                recipients: None,
                audience: Some(audience),
                subject_override: None,
                reply_to_address: None,
                dry_run: Some(false),
            },
        )
        .await
    }

    async fn resolve_recipients(
        &self,
        pool: &PgPool,
        template_id: &str,
        allow_arbitrary_addresses: bool,
        recipients: Option<&EmailRecipientsRequest>,
        audience: Option<&EmailAudienceRequest>,
    ) -> Result<ResolvedRecipients, ApiError> {
        let mut deduped = BTreeMap::<String, ResolvedRecipient>::new();

        if let Some(recipients) = recipients {
            for recipient in self.fetch_explicit_users(pool, &recipients.users).await? {
                deduped.entry(recipient.email.clone()).or_insert(recipient);
            }

            if !recipients.emails.is_empty() && !allow_arbitrary_addresses {
                return Err(ApiError::BadRequest);
            }

            for email in &recipients.emails {
                let Some(normalized) = normalize_email(email) else {
                    continue;
                };
                deduped
                    .entry(normalized.clone())
                    .or_insert(ResolvedRecipient {
                        user_id: None,
                        email: normalized,
                        display_name: None,
                        source: "explicit_email".to_string(),
                    });
            }
        }

        if let Some(audience) = audience {
            for recipient in resolve_audience(pool, audience).await? {
                deduped.entry(recipient.email.clone()).or_insert(recipient);
            }
        }

        if deduped.is_empty() {
            return Err(ApiError::BadRequest);
        }

        let template = find_template(template_id).ok_or(ApiError::BadRequest)?;
        let mut deliverable = Vec::new();
        let mut suppressed = Vec::new();

        for recipient in deduped.into_values() {
            if !template.is_transactional
                && is_suppressed(pool, template.category, &recipient.email).await?
            {
                suppressed.push(recipient);
            } else {
                deliverable.push(recipient);
            }
        }

        Ok(ResolvedRecipients {
            deliverable,
            suppressed,
        })
    }

    async fn fetch_explicit_users(
        &self,
        pool: &PgPool,
        user_ids: &[String],
    ) -> Result<Vec<ResolvedRecipient>, ApiError> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, ExplicitUserRow>(
            r#"
            select
                id,
                coalesce(email::text, '') as email,
                display_name
            from identity.users
            where id = any($1)
            "#,
        )
        .bind(user_ids)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                normalize_email(&row.email).map(|email| ResolvedRecipient {
                    user_id: Some(row.id),
                    email,
                    display_name: Some(row.display_name),
                    source: "explicit_user".to_string(),
                })
            })
            .collect())
    }

    pub async fn list_outbox(
        &self,
        pool: &PgPool,
        query: &ListEmailOutboxQuery,
    ) -> Result<Vec<EmailOutboxListItem>, ApiError> {
        self.ensure_available()?;
        sqlx::query_as::<_, EmailOutboxListItem>(
            r#"
            select
                o.id::text as id,
                o.template_id,
                o.category,
                o.is_transactional,
                o.request_source,
                o.status,
                o.attempt_count,
                o.queued_at,
                o.sent_at,
                o.failed_at,
                count(r.*)::bigint as recipient_count,
                count(*) filter (where r.delivery_status = 'sent')::bigint as delivered_count,
                count(*) filter (where r.delivery_status = 'suppressed')::bigint as suppressed_count
            from email.outbox o
            left join email.outbox_recipients r on r.outbox_id = o.id
            where ($1::text is null or o.status = $1)
              and ($2::text is null or o.template_id = $2)
            group by o.id
            order by o.queued_at desc
            limit $3 offset $4
            "#,
        )
        .bind(query.status.as_deref())
        .bind(query.template_id.as_deref())
        .bind(query.limit.unwrap_or(50).clamp(1, 200))
        .bind(query.offset.unwrap_or(0).max(0))
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)
    }

    pub async fn get_outbox_detail(
        &self,
        pool: &PgPool,
        id: &str,
    ) -> Result<EmailOutboxDetailResponse, ApiError> {
        self.ensure_available()?;

        let row = sqlx::query_as::<_, OutboxDetailRow>(
            r#"
            select
                id::text as id,
                template_id,
                category,
                is_transactional,
                request_source,
                subject_override,
                reply_to_address,
                payload,
                audience_filter,
                status,
                attempt_count,
                next_attempt_at,
                last_error,
                provider,
                provider_message_id,
                queued_at,
                sent_at,
                failed_at
            from email.outbox
            where id::text = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)?
        .ok_or(ApiError::BadRequest)?;

        let recipients = sqlx::query_as::<_, OutboxRecipientRow>(
            r#"
            select
                id::text as id,
                user_id,
                email::text as email,
                display_name,
                source,
                suppression_reason,
                delivery_status,
                provider_message_id,
                sent_at,
                failed_at,
                last_error
            from email.outbox_recipients
            where outbox_id::text = $1
            order by created_at asc
            "#,
        )
        .bind(id)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

        Ok(EmailOutboxDetailResponse {
            id: row.id,
            template_id: row.template_id,
            category: row.category,
            is_transactional: row.is_transactional,
            request_source: row.request_source,
            subject_override: row.subject_override,
            reply_to_address: row.reply_to_address,
            payload: row.payload,
            audience_filter: row.audience_filter,
            status: row.status,
            attempt_count: row.attempt_count,
            next_attempt_at: row.next_attempt_at,
            last_error: row.last_error,
            provider: row.provider,
            provider_message_id: row.provider_message_id,
            queued_at: row.queued_at,
            sent_at: row.sent_at,
            failed_at: row.failed_at,
            recipients: recipients
                .into_iter()
                .map(|recipient| EmailOutboxRecipientResponse {
                    id: recipient.id,
                    user_id: recipient.user_id,
                    email: recipient.email,
                    display_name: recipient.display_name,
                    source: recipient.source,
                    suppression_reason: recipient.suppression_reason,
                    delivery_status: recipient.delivery_status,
                    provider_message_id: recipient.provider_message_id,
                    sent_at: recipient.sent_at,
                    failed_at: recipient.failed_at,
                    last_error: recipient.last_error,
                })
                .collect(),
        })
    }

    pub async fn unsubscribe(
        &self,
        pool: &PgPool,
        token: &str,
    ) -> Result<EmailUnsubscribeResponse, ApiError> {
        self.ensure_available()?;
        let secret = self
            .config
            .unsubscribe_secret
            .as_deref()
            .ok_or(ApiError::ServiceUnavailable)?;
        let claims = verify_unsubscribe_token(secret, token)?;
        if claims.category == "transactional" {
            return Err(ApiError::BadRequest);
        }
        create_suppression(pool, &claims, "token").await?;
        Ok(EmailUnsubscribeResponse {
            category: claims.category,
            email: claims.email,
            status: "unsubscribed".to_string(),
        })
    }

    pub async fn resubscribe(
        &self,
        pool: &PgPool,
        request: &EmailResubscribeRequest,
    ) -> Result<EmailSuppressionRecordResponse, ApiError> {
        self.ensure_available()?;
        revoke_suppression(pool, &request.category, &request.email).await?;
        Ok(EmailSuppressionRecordResponse {
            category: request.category.clone(),
            email: request.email.clone(),
            status: "active".to_string(),
        })
    }

    pub async fn process_pending_batch(&self, pool: &PgPool) -> Result<usize, ApiError> {
        self.ensure_available()?;
        let jobs = outbox::claim_pending_jobs(pool, self.config.worker_batch_size).await?;
        let mut processed = 0usize;

        for job in jobs {
            let template = find_template(&job.template_id).ok_or(ApiError::BadRequest)?;
            let recipients = outbox::fetch_pending_recipients(pool, job.id).await?;
            let mut any_send = false;
            let mut first_message_id: Option<String> = None;

            for recipient in recipients {
                if recipient.delivery_status == "suppressed" {
                    continue;
                }
                outbox::mark_recipient_processing(pool, recipient.id).await?;
                let rendered = render_template(
                    template,
                    &job.payload,
                    self.config.unsubscribe_base_url.as_deref(),
                    self.config.unsubscribe_secret.as_deref(),
                    Some(&recipient.email),
                    recipient.user_id.as_deref(),
                )?;

                match self
                    .mailer
                    .send_email(
                        &recipient.email,
                        job.subject_override.as_deref().unwrap_or(&rendered.subject),
                        &rendered.html,
                        &rendered.text,
                        job.reply_to_address.as_deref(),
                    )
                    .await
                {
                    Ok(message_id) => {
                        if first_message_id.is_none() {
                            first_message_id = message_id.clone();
                        }
                        outbox::mark_recipient_sent(pool, recipient.id, message_id.as_deref())
                            .await?;
                        any_send = true;
                    }
                    Err(ApiError::ServiceUnavailable) => return Err(ApiError::ServiceUnavailable),
                    Err(_) => {
                        outbox::mark_recipient_failed(pool, recipient.id, "delivery_failed")
                            .await?;
                    }
                }
            }

            if any_send {
                outbox::finalize_outbox_status(pool, job.id, first_message_id.as_deref()).await?;
            } else {
                outbox::finalize_outbox_status(pool, job.id, None).await?;
            }
            processed += 1;
        }

        Ok(processed)
    }

    pub async fn pending_count(&self, pool: &PgPool) -> Result<i64, ApiError> {
        outbox::pending_count(pool).await
    }

    fn ensure_available(&self) -> Result<(), ApiError> {
        if self.is_available() {
            Ok(())
        } else {
            Err(ApiError::ServiceUnavailable)
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedRecipients {
    deliverable: Vec<ResolvedRecipient>,
    suppressed: Vec<ResolvedRecipient>,
}

#[derive(sqlx::FromRow)]
struct ExplicitUserRow {
    id: String,
    email: String,
    display_name: String,
}

#[derive(sqlx::FromRow)]
struct OutboxDetailRow {
    id: String,
    template_id: String,
    category: String,
    is_transactional: bool,
    request_source: String,
    subject_override: Option<String>,
    reply_to_address: Option<String>,
    payload: Value,
    audience_filter: Option<Value>,
    status: String,
    attempt_count: i32,
    next_attempt_at: DateTime<Utc>,
    last_error: Option<String>,
    provider: Option<String>,
    provider_message_id: Option<String>,
    queued_at: DateTime<Utc>,
    sent_at: Option<DateTime<Utc>>,
    failed_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct OutboxRecipientRow {
    id: String,
    user_id: Option<String>,
    email: String,
    display_name: Option<String>,
    source: String,
    suppression_reason: Option<String>,
    delivery_status: String,
    provider_message_id: Option<String>,
    sent_at: Option<DateTime<Utc>>,
    failed_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

pub fn actor_from_context(
    current_user: Option<&CurrentUser>,
    current_service_account: Option<&CurrentServiceAccount>,
    resolved_actor_id: Option<String>,
    request_source: &str,
) -> EmailActor {
    EmailActor {
        actor_id: resolved_actor_id,
        user_id: current_user.map(|user| user.id.clone()),
        service_account_id: current_service_account.map(|account| account.id.clone()),
        request_source: request_source.to_string(),
    }
}

fn normalize_optional_email(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

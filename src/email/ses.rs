use aws_sdk_sesv2::{
    error::ProvideErrorMetadata,
    operation::RequestId,
    Client,
    types::{Body, Content, Destination, EmailContent, Message},
};

use crate::errors::ApiError;

use super::config::EmailConfig;

#[derive(Clone)]
pub struct SesMailer {
    client: Option<Client>,
    config: EmailConfig,
}

impl SesMailer {
    pub fn disabled(config: EmailConfig) -> Self {
        Self {
            client: None,
            config,
        }
    }

    pub async fn from_config(config: EmailConfig) -> Self {
        if !config.transport_enabled() {
            return Self::disabled(config);
        }

        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
        if let Some(region) = config.region.as_deref() {
            loader = loader.region(aws_config::Region::new(region.to_string()));
        }
        let shared = loader.load().await;
        let mut builder = aws_sdk_sesv2::config::Builder::from(&shared);
        if let Some(endpoint) = config.ses_endpoint.as_deref() {
            builder = builder.endpoint_url(endpoint);
        }

        let client = Client::from_conf(builder.build());
        Self {
            client: Some(client),
            config,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.client.is_some() && self.config.transport_enabled()
    }

    pub async fn send_email(
        &self,
        to_email: &str,
        subject: &str,
        html: &str,
        text: &str,
        reply_to_address: Option<&str>,
    ) -> Result<Option<String>, ApiError> {
        let Some(client) = self.client.as_ref() else {
            return Err(ApiError::ServiceUnavailable);
        };

        let from_address = match (
            self.config.from_name.as_deref(),
            self.config.from_address.as_deref(),
        ) {
            (Some(name), Some(address)) => format!("{name} <{address}>"),
            (None, Some(address)) => address.to_string(),
            _ => return Err(ApiError::ServiceUnavailable),
        };

        let mut request = client
            .send_email()
            .from_email_address(from_address)
            .destination(Destination::builder().to_addresses(to_email).build())
            .content(
                EmailContent::builder()
                    .simple(
                        Message::builder()
                            .subject(
                                Content::builder()
                                    .data(subject)
                                    .charset("UTF-8")
                                    .build()
                                    .map_err(|_| ApiError::Internal)?,
                            )
                            .body(
                                Body::builder()
                                    .html(
                                        Content::builder()
                                            .data(html)
                                            .charset("UTF-8")
                                            .build()
                                            .map_err(|_| ApiError::Internal)?,
                                    )
                                    .text(
                                        Content::builder()
                                            .data(text)
                                            .charset("UTF-8")
                                            .build()
                                            .map_err(|_| ApiError::Internal)?,
                                    )
                                    .build(),
                            )
                            .build(),
                    )
                    .build(),
            );

        if let Some(reply_to) = reply_to_address.or(self.config.reply_to_address.as_deref()) {
            request = request.reply_to_addresses(reply_to);
        }

        let output = request.send().await.map_err(|error| {
            let aws_error_code = error.code().map(str::to_string);
            let aws_error_message = error.message().map(str::to_string);
            let request_id = error.request_id().map(str::to_string);

            tracing::error!(
                error = %error,
                aws_error_code = aws_error_code.as_deref().unwrap_or("unknown"),
                aws_error_message = aws_error_message.as_deref().unwrap_or("unknown"),
                aws_request_id = request_id.as_deref().unwrap_or("unknown"),
                "ses send_email failed"
            );
            ApiError::Internal
        })?;

        Ok(output.message_id)
    }
}

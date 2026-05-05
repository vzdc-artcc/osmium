use maud::html;
use serde_json::Value;

use crate::email::rsx::components::{callout, EmailLayout};
use crate::email::rsx::text::TextBuilder;
use crate::email::templates::RenderedEmail;
use crate::errors::ApiError;

use super::RsxTemplate;

fn required_string(payload: &Value, key: &str) -> Result<String, ApiError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .ok_or(ApiError::BadRequest)
}

fn optional_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

pub struct AppointmentScheduledTemplate;

impl RsxTemplate for AppointmentScheduledTemplate {
    fn id(&self) -> &'static str {
        "training.appointment_scheduled"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let student_name = required_string(payload, "student_name")?;
        let trainer_name = required_string(payload, "trainer_name")?;
        let appointment_start = required_string(payload, "appointment_start")?;
        let details_url = optional_string(payload, "details_url");

        let subject = format!("Training appointment scheduled with {trainer_name}");

        let body = html! {
            p {
                "A training appointment has been scheduled between "
                strong { (student_name) }
                " and "
                strong { (trainer_name) }
                "."
            }
            (callout(html! {
                p { strong { "Date/Time:" } " " (appointment_start) }
            }))
            p {
                "Please ensure you are prepared and available at the scheduled time. "
                "If you need to cancel or reschedule, please do so as soon as possible."
            }
        };

        let cta = details_url.as_deref().map(|url| ("View appointment", url));

        let html = EmailLayout::new(&subject)
            .preheader(&format!("Training scheduled for {appointment_start}"))
            .heading("Appointment Scheduled")
            .unsubscribe_link(unsubscribe_link)
            .render(body, cta)
            .into_string();

        let mut text = TextBuilder::new()
            .line(&format!("A training appointment has been scheduled between {student_name} and {trainer_name}."))
            .blank()
            .line(&format!("Date/Time: {appointment_start}"))
            .blank()
            .line("Please ensure you are prepared and available at the scheduled time.");

        if let Some(url) = details_url.as_deref() {
            text = text.link("View appointment", url);
        }

        let text = text.optional_unsubscribe(unsubscribe_link).build();

        Ok(RenderedEmail { subject, html, text })
    }
}

pub struct AppointmentCanceledTemplate;

impl RsxTemplate for AppointmentCanceledTemplate {
    fn id(&self) -> &'static str {
        "training.appointment_canceled"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let student_name = required_string(payload, "student_name")?;
        let trainer_name = required_string(payload, "trainer_name")?;
        let appointment_start = required_string(payload, "appointment_start")?;
        let reason = optional_string(payload, "reason");

        let subject = "Training appointment canceled".to_string();

        let body = html! {
            p {
                "The training appointment between "
                strong { (student_name) }
                " and "
                strong { (trainer_name) }
                " has been canceled."
            }
            (callout(html! {
                p { strong { "Original Date/Time:" } " " (appointment_start) }
                @if let Some(ref r) = reason {
                    p { strong { "Reason:" } " " (r) }
                }
            }))
            p {
                "Please coordinate with your trainer to schedule a new appointment."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader("Training appointment canceled")
            .heading("Appointment Canceled")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let mut text = TextBuilder::new()
            .line(&format!("The training appointment between {student_name} and {trainer_name} has been canceled."))
            .blank()
            .line(&format!("Original Date/Time: {appointment_start}"));

        if let Some(ref r) = reason {
            text = text.line(&format!("Reason: {r}"));
        }

        let text = text
            .blank()
            .line("Please coordinate with your trainer to schedule a new appointment.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail { subject, html, text })
    }
}

pub struct AppointmentUpdatedTemplate;

impl RsxTemplate for AppointmentUpdatedTemplate {
    fn id(&self) -> &'static str {
        "training.appointment_updated"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let student_name = required_string(payload, "student_name")?;
        let trainer_name = required_string(payload, "trainer_name")?;
        let appointment_start = required_string(payload, "appointment_start")?;
        let details_url = optional_string(payload, "details_url");

        let subject = "Training appointment updated".to_string();

        let body = html! {
            p {
                "The training appointment between "
                strong { (student_name) }
                " and "
                strong { (trainer_name) }
                " has been updated."
            }
            (callout(html! {
                p { strong { "New Date/Time:" } " " (appointment_start) }
            }))
            p {
                "Please review the updated appointment details and ensure you are available."
            }
        };

        let cta = details_url.as_deref().map(|url| ("View appointment", url));

        let html = EmailLayout::new(&subject)
            .preheader(&format!("Training rescheduled to {appointment_start}"))
            .heading("Appointment Updated")
            .unsubscribe_link(unsubscribe_link)
            .render(body, cta)
            .into_string();

        let mut text = TextBuilder::new()
            .line(&format!("The training appointment between {student_name} and {trainer_name} has been updated."))
            .blank()
            .line(&format!("New Date/Time: {appointment_start}"));

        if let Some(url) = details_url.as_deref() {
            text = text.link("View appointment", url);
        }

        let text = text.optional_unsubscribe(unsubscribe_link).build();

        Ok(RenderedEmail { subject, html, text })
    }
}

pub struct AppointmentWarningTemplate;

impl RsxTemplate for AppointmentWarningTemplate {
    fn id(&self) -> &'static str {
        "training.appointment_warning"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let _student_name = required_string(payload, "student_name")?;
        let trainer_name = required_string(payload, "trainer_name")?;
        let appointment_start = required_string(payload, "appointment_start")?;
        let warning_message = optional_string(payload, "warning_message");

        let subject = "Training appointment reminder".to_string();

        let body = html! {
            p {
                "This is a reminder about your upcoming training appointment with "
                strong { (trainer_name) }
                "."
            }
            (callout(html! {
                p { strong { "Date/Time:" } " " (appointment_start) }
            }))
            @if let Some(ref msg) = warning_message {
                p { (msg) }
            }
            p {
                "Please ensure you are prepared and ready for your session."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader(&format!("Reminder: Training with {trainer_name}"))
            .heading("Appointment Reminder")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let mut text = TextBuilder::new()
            .line(&format!("This is a reminder about your upcoming training appointment with {trainer_name}."))
            .blank()
            .line(&format!("Date/Time: {appointment_start}"));

        if let Some(ref msg) = warning_message {
            text = text.blank().line(msg);
        }

        let text = text
            .blank()
            .line("Please ensure you are prepared and ready for your session.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail { subject, html, text })
    }
}

pub struct SessionCreatedTemplate;

impl RsxTemplate for SessionCreatedTemplate {
    fn id(&self) -> &'static str {
        "training.session_created"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let student_name = required_string(payload, "student_name")?;
        let trainer_name = required_string(payload, "trainer_name")?;
        let session_date = required_string(payload, "session_date")?;
        let position = optional_string(payload, "position");
        let details_url = optional_string(payload, "details_url");

        let subject = "Training session recorded".to_string();

        let body = html! {
            p {
                "A training session has been recorded for "
                strong { (student_name) }
                " with "
                strong { (trainer_name) }
                "."
            }
            (callout(html! {
                p { strong { "Date:" } " " (session_date) }
                @if let Some(ref pos) = position {
                    p { strong { "Position:" } " " (pos) }
                }
            }))
        };

        let cta = details_url.as_deref().map(|url| ("View session", url));

        let html = EmailLayout::new(&subject)
            .preheader(&format!("Training session recorded on {session_date}"))
            .heading("Session Recorded")
            .unsubscribe_link(unsubscribe_link)
            .render(body, cta)
            .into_string();

        let mut text = TextBuilder::new()
            .line(&format!("A training session has been recorded for {student_name} with {trainer_name}."))
            .blank()
            .line(&format!("Date: {session_date}"));

        if let Some(ref pos) = position {
            text = text.line(&format!("Position: {pos}"));
        }

        if let Some(url) = details_url.as_deref() {
            text = text.link("View session", url);
        }

        let text = text.optional_unsubscribe(unsubscribe_link).build();

        Ok(RenderedEmail { subject, html, text })
    }
}

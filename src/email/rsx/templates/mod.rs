mod announcement;
mod broadcast;
mod event;
mod feedback;
mod incident;
mod loa;
mod progression;
mod solo;
mod system;
mod training;
mod visitor;

use serde_json::Value;

use crate::email::templates::RenderedEmail;
use crate::errors::ApiError;

pub trait RsxTemplate: Send + Sync {
    fn id(&self) -> &'static str;
    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError>;
}

static RSX_TEMPLATES: &[&dyn RsxTemplate] = &[
    // System
    &system::SystemTestTemplate,
    // Announcements
    &announcement::AnnouncementTemplate,
    // Events
    &event::EventPositionPublishedTemplate,
    &event::EventReminderTemplate,
    // LOA
    &loa::LoaApprovedTemplate,
    &loa::LoaDeniedTemplate,
    &loa::LoaDeletedTemplate,
    &loa::LoaExpiredTemplate,
    // Training
    &training::AppointmentScheduledTemplate,
    &training::AppointmentCanceledTemplate,
    &training::AppointmentUpdatedTemplate,
    &training::AppointmentWarningTemplate,
    &training::SessionCreatedTemplate,
    // Visitor
    &visitor::VisitorAcceptedTemplate,
    &visitor::VisitorRejectedTemplate,
    // Solo
    &solo::SoloAddedTemplate,
    &solo::SoloDeletedTemplate,
    &solo::SoloExpiredTemplate,
    // Feedback
    &feedback::NewFeedbackTemplate,
    // Incident
    &incident::IncidentClosedTemplate,
    // Broadcast
    &broadcast::BroadcastPostedTemplate,
    // Progression
    &progression::ProgressionAssignedTemplate,
    &progression::ProgressionRemovedTemplate,
];

pub fn find_rsx_template(id: &str) -> Option<&'static dyn RsxTemplate> {
    RSX_TEMPLATES.iter().find(|t| t.id() == id).copied()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rsx_registry_contains_all_templates() {
        let expected = [
            "system.test_email",
            "announcements.generic",
            "events.position_published",
            "events.reminder",
            "loa.approved",
            "loa.denied",
            "loa.deleted",
            "loa.expired",
            "training.appointment_scheduled",
            "training.appointment_canceled",
            "training.appointment_updated",
            "training.appointment_warning",
            "training.session_created",
            "visitor.accepted",
            "visitor.rejected",
            "solo.added",
            "solo.deleted",
            "solo.expired",
            "feedback.new",
            "incident.closed",
            "broadcast.posted",
            "progression.assigned",
            "progression.removed",
        ];

        for id in expected {
            assert!(find_rsx_template(id).is_some(), "missing RSX template: {id}");
        }

        assert_eq!(RSX_TEMPLATES.len(), expected.len(), "template count mismatch");
    }

    #[test]
    fn system_test_renders_with_message() {
        let template = find_rsx_template("system.test_email").unwrap();
        let result = template
            .render(&json!({"message": "Hello from RSX"}), None)
            .unwrap();

        assert_eq!(result.subject, "Osmium email transport test");
        assert!(result.html.contains("Hello from RSX"));
        assert!(result.html.contains("vZDC"));
        assert!(result.html.contains("Washington ARTCC"));
        assert!(result.html.contains("Sent by vZDC."));
        assert!(!result.html.contains("Unsubscribe from this category"));
        assert!(result.text.contains("Hello from RSX"));
    }

    #[test]
    fn system_test_renders_with_requested_by() {
        let template = find_rsx_template("system.test_email").unwrap();
        let result = template
            .render(
                &json!({"message": "Test", "requested_by": "admin@example.com"}),
                None,
            )
            .unwrap();

        assert!(result.html.contains("admin@example.com"));
        assert!(result.text.contains("Requested by: admin@example.com"));
    }

    #[test]
    fn announcement_renders_with_markdown() {
        let template = find_rsx_template("announcements.generic").unwrap();
        let result = template
            .render(
                &json!({
                    "headline": "Status Update",
                    "body_markdown": "Hello team\n\nThis is important."
                }),
                None,
            )
            .unwrap();

        assert_eq!(result.subject, "Status Update");
        assert!(result.html.contains("<p>"));
        assert!(result.text.contains("Hello team"));
    }

    #[test]
    fn event_reminder_renders_with_location() {
        let template = find_rsx_template("events.reminder").unwrap();
        let result = template
            .render(
                &json!({
                    "event_title": "ZDC FNO",
                    "starts_at": "2026-05-10T20:00:00Z",
                    "details_url": "https://example.com/events/123",
                    "location": "KDCA Ground"
                }),
                Some("https://unsub.example.com/token"),
            )
            .unwrap();

        assert!(result.html.contains("ZDC FNO"));
        assert!(result.html.contains("KDCA Ground"));
        assert!(result.html.contains("Unsubscribe"));
        assert!(result.html.contains("href=\"https://example.com/events/123\""));
        assert!(result.html.contains("href=\"https://unsub.example.com/token\""));
        assert!(result.html.contains("#500e0e"));
        assert!(result.html.contains("vZDC"));
        assert!(result.text.contains("Location: KDCA Ground"));
    }
}

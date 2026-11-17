mod announcement;
mod broadcast;
mod event;
mod feedback;
mod incident;
mod loa;
mod progression;
mod roster;
mod solo;
mod system;
mod training;
mod training_assignment;
mod visitor;

use serde_json::Value;

use crate::email::branding::EmailTheme;
use crate::email::templates::RenderedEmail;
use crate::errors::ApiError;

pub trait RsxTemplate: Send + Sync {
    fn id(&self) -> &'static str;
    fn render(
        &self,
        payload: &Value,
        theme: &EmailTheme,
        unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError>;
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
    &training_assignment::OTS_ASSIGNED_STUDENT,
    &training_assignment::OTS_ASSIGNED_INSTRUCTOR,
    &training_assignment::OTS_UNASSIGNED_STUDENT,
    &training_assignment::OTS_UNASSIGNED_INSTRUCTOR,
    &training_assignment::OTS_DELETED,
    &training_assignment::ASSIGNMENT_CREATED,
    &training_assignment::ASSIGNMENT_PRIMARY_TRAINER,
    &training_assignment::ASSIGNMENT_OTHER_TRAINER,
    &training_assignment::ASSIGNMENT_UPDATED,
    &training_assignment::ASSIGNMENT_TRAINER_REMOVED,
    &training_assignment::ASSIGNMENT_RELEASED,
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
    // Roster
    &roster::RosterRemovedTemplate,
];

pub fn find_rsx_template(id: &str) -> Option<&'static dyn RsxTemplate> {
    RSX_TEMPLATES.iter().find(|t| t.id() == id).copied()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::models::EmailBranding;

    fn test_branding() -> EmailBranding {
        EmailBranding {
            brand_name: "vZDC".to_string(),
            tagline: "Washington ARTCC".to_string(),
            footer_text: "Sent by vZDC.".to_string(),
            logo_file_id: None,
            header_background_color: "#500e0e".to_string(),
            header_text_color: "#ededf5".to_string(),
            page_background_color: "#f1f0f6".to_string(),
            panel_background_color: "#ffffff".to_string(),
            text_color: "#1f2430".to_string(),
            heading_color: "#500e0e".to_string(),
            link_color: "#500e0e".to_string(),
            accent_color: "#500e0e".to_string(),
            button_background_color: "#500e0e".to_string(),
            button_text_color: "#ededf5".to_string(),
            heading_font_family: "roboto_sans".to_string(),
            body_font_family: "roboto_sans".to_string(),
            font_size_scale: "medium".to_string(),
            corner_style: "soft".to_string(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn test_theme(branding: &EmailBranding) -> EmailTheme<'_> {
        EmailTheme::new(branding, None)
    }

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
            "training.ots_assigned_student",
            "training.ots_assigned_instructor",
            "training.ots_unassigned_student",
            "training.ots_unassigned_instructor",
            "training.ots_deleted",
            "training.assignment_created",
            "training.assignment_primary_trainer",
            "training.assignment_other_trainer",
            "training.assignment_updated",
            "training.assignment_trainer_removed",
            "training.assignment_released",
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
            "roster.removed",
        ];

        for id in expected {
            assert!(
                find_rsx_template(id).is_some(),
                "missing RSX template: {id}"
            );
        }

        assert_eq!(
            RSX_TEMPLATES.len(),
            expected.len(),
            "template count mismatch"
        );
    }

    #[test]
    fn training_notices_render_their_names_and_refuse_a_missing_field() {
        let branding = test_branding();
        let theme = test_theme(&branding);
        let payload = json!({
            "change": "created",
            "student_name": "Sam Student",
            "student_cid": "1260002",
            "student_rating": "S2",
            "instructor_name": "Ivy Instructor",
            "primary_trainer_name": "Pat Primary",
            "other_trainer_names": "Olly Other",
        });
        // (template, text it must contain, whether an empty payload is refused)
        let cases = [
            ("training.ots_assigned_student", "Ivy Instructor", true),
            ("training.ots_assigned_instructor", "Sam Student", true),
            (
                "training.ots_unassigned_student",
                "Dear Sam Student,",
                false,
            ),
            ("training.ots_unassigned_instructor", "Sam Student", true),
            ("training.ots_deleted", "Sam Student", true),
            ("training.assignment_created", "Olly Other", true),
            (
                "training.assignment_primary_trainer",
                "Sam Student (S2)",
                true,
            ),
            ("training.assignment_other_trainer", "Pat Primary", true),
            ("training.assignment_updated", "Dear Sam Student,", false),
            ("training.assignment_released", "Dear Sam Student,", false),
        ];
        for (id, expected, refuses_empty) in cases {
            let template = find_rsx_template(id).unwrap();
            let result = template.render(&payload, &theme, None).unwrap();
            assert!(!result.subject.is_empty(), "{id} subject");
            assert!(result.html.contains(expected), "{id} html has {expected}");
            assert!(result.text.contains(expected), "{id} text has {expected}");
            assert_eq!(
                template.render(&json!({}), &theme, None).is_err(),
                refuses_empty,
                "{id} empty payload"
            );
        }

        // A blank recipient name falls back to a generic greeting.
        let updated = find_rsx_template("training.assignment_updated").unwrap();
        let blank = updated
            .render(&json!({ "student_name": " " }), &theme, None)
            .unwrap();
        assert!(blank.text.starts_with("Hello,"), "{}", blank.text);
    }

    #[test]
    fn training_trainer_notices_use_the_legacy_subject_for_each_change() {
        let branding = test_branding();
        let theme = test_theme(&branding);
        let cases = [
            (
                "training.assignment_primary_trainer",
                "created",
                "Training Assignment - Primary",
            ),
            (
                "training.assignment_primary_trainer",
                "updated",
                "Training Assignment Updated - Primary Trainer",
            ),
            (
                "training.assignment_other_trainer",
                "created",
                "Training Request Fulfilled",
            ),
            (
                "training.assignment_other_trainer",
                "updated",
                "Training Assignment Updated - Trainer Added",
            ),
            (
                "training.assignment_trainer_removed",
                "updated",
                "Training Assignment Updated - Trainer Removed",
            ),
            (
                "training.assignment_trainer_removed",
                "deleted",
                "Training Assignment Released",
            ),
            (
                "training.assignment_trainer_removed",
                "release_approved",
                "Release Request Approved",
            ),
        ];
        for (id, change, subject) in cases {
            let payload = json!({
                "change": change,
                "student_name": "Sam Student",
                "student_cid": "1260002",
                "primary_trainer_name": "Pat Primary",
            });
            let template = find_rsx_template(id).unwrap();
            let result = template.render(&payload, &theme, None).unwrap();
            assert_eq!(result.subject, subject, "{id} {change}");
            let mut unknown = payload.clone();
            unknown["change"] = json!("archived");
            assert!(
                template.render(&unknown, &theme, None).is_err(),
                "{id} refuses an unknown change"
            );
        }

        let released = find_rsx_template("training.assignment_released").unwrap();
        let deleted = released
            .render(&json!({ "student_name": "Sam Student" }), &theme, None)
            .unwrap();
        assert_eq!(deleted.subject, "Training Assignment Deleted");
        assert!(!deleted.text.contains("release request has been approved"));
        let approved = released
            .render(
                &json!({ "student_name": "Sam Student", "release_request_approved": true }),
                &theme,
                None,
            )
            .unwrap();
        assert_eq!(approved.subject, "Release Request Approved");
        assert!(
            approved
                .text
                .contains("Your release request has been approved.")
        );
        assert!(
            released
                .render(&json!({ "release_request_approved": "yes" }), &theme, None)
                .is_err(),
            "a non-boolean flag is refused"
        );
    }

    #[test]
    fn system_test_renders_with_message() {
        let branding = test_branding();
        let theme = test_theme(&branding);
        let template = find_rsx_template("system.test_email").unwrap();
        let result = template
            .render(&json!({"message": "Hello from RSX"}), &theme, None)
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
        let branding = test_branding();
        let theme = test_theme(&branding);
        let template = find_rsx_template("system.test_email").unwrap();
        let result = template
            .render(
                &json!({"message": "Test", "requested_by": "admin@example.com"}),
                &theme,
                None,
            )
            .unwrap();

        assert!(result.html.contains("admin@example.com"));
        assert!(result.text.contains("Requested by: admin@example.com"));
    }

    #[test]
    fn announcement_renders_with_markdown() {
        let branding = test_branding();
        let theme = test_theme(&branding);
        let template = find_rsx_template("announcements.generic").unwrap();
        let result = template
            .render(
                &json!({
                    "headline": "Status Update",
                    "body_markdown": "Hello team\n\nThis is important."
                }),
                &theme,
                None,
            )
            .unwrap();

        assert_eq!(result.subject, "Status Update");
        assert!(result.html.contains("<p>"));
        assert!(result.text.contains("Hello team"));
    }

    #[test]
    fn event_reminder_renders_with_location() {
        let branding = test_branding();
        let theme = test_theme(&branding);
        let template = find_rsx_template("events.reminder").unwrap();
        let result = template
            .render(
                &json!({
                    "event_title": "ZDC FNO",
                    "starts_at": "2026-05-10T20:00:00Z",
                    "details_url": "https://example.com/events/123",
                    "location": "KDCA Ground"
                }),
                &theme,
                Some("https://unsub.example.com/token"),
            )
            .unwrap();

        assert!(result.html.contains("ZDC FNO"));
        assert!(result.html.contains("KDCA Ground"));
        assert!(result.html.contains("Unsubscribe"));
        assert!(
            result
                .html
                .contains("href=\"https://example.com/events/123\"")
        );
        assert!(
            result
                .html
                .contains("href=\"https://unsub.example.com/token\"")
        );
        assert!(
            result
                .html
                .contains(branding.header_background_color.as_str())
        );
        assert!(result.html.contains("vZDC"));
        assert!(result.text.contains("Location: KDCA Ground"));
    }
}

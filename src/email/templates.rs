use serde_json::{Value, json};

use super::suppression::build_unsubscribe_link;

#[derive(Debug, Clone)]
pub struct RenderedEmail {
    pub subject: String,
    pub html: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct TemplateDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub is_transactional: bool,
    pub allow_arbitrary_addresses: bool,
    pub respect_user_event_pref: bool,
    pub payload_schema: fn() -> Value,
}

pub fn registry() -> &'static [TemplateDefinition] {
    &[
        TemplateDefinition {
            id: "announcements.generic",
            name: "Generic Announcement",
            category: "announcements",
            description: "Generic formatted announcement email",
            is_transactional: false,
            allow_arbitrary_addresses: true,
            respect_user_event_pref: false,
            payload_schema: announcement_schema,
        },
        TemplateDefinition {
            id: "events.position_published",
            name: "Event Position Published",
            category: "event_notifications",
            description: "Event position publication notice",
            is_transactional: false,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: true,
            payload_schema: event_position_published_schema,
        },
        TemplateDefinition {
            id: "events.reminder",
            name: "Event Reminder",
            category: "event_notifications",
            description: "Reminder for an upcoming event",
            is_transactional: false,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: true,
            payload_schema: event_reminder_schema,
        },
        TemplateDefinition {
            id: "system.test_email",
            name: "System Test Email",
            category: "transactional",
            description: "Simple diagnostic email for SES connectivity",
            is_transactional: true,
            allow_arbitrary_addresses: true,
            respect_user_event_pref: false,
            payload_schema: system_test_schema,
        },
        // LOA templates
        TemplateDefinition {
            id: "loa.approved",
            name: "LOA Approved",
            category: "transactional",
            description: "Leave of Absence approval notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: loa_approved_schema,
        },
        TemplateDefinition {
            id: "loa.denied",
            name: "LOA Denied",
            category: "transactional",
            description: "Leave of Absence denial notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: loa_denied_schema,
        },
        TemplateDefinition {
            id: "loa.deleted",
            name: "LOA Deleted",
            category: "transactional",
            description: "Leave of Absence deletion notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: loa_deleted_schema,
        },
        TemplateDefinition {
            id: "loa.expired",
            name: "LOA Expired",
            category: "transactional",
            description: "Leave of Absence expiration notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: loa_expired_schema,
        },
        // Training templates
        TemplateDefinition {
            id: "training.appointment_scheduled",
            name: "Appointment Scheduled",
            category: "training",
            description: "Training appointment scheduled notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: appointment_scheduled_schema,
        },
        TemplateDefinition {
            id: "training.appointment_canceled",
            name: "Appointment Canceled",
            category: "training",
            description: "Training appointment cancellation notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: appointment_canceled_schema,
        },
        TemplateDefinition {
            id: "training.appointment_updated",
            name: "Appointment Updated",
            category: "training",
            description: "Training appointment update notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: appointment_updated_schema,
        },
        TemplateDefinition {
            id: "training.appointment_warning",
            name: "Appointment Reminder",
            category: "training",
            description: "Training appointment reminder notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: appointment_warning_schema,
        },
        TemplateDefinition {
            id: "training.session_created",
            name: "Session Recorded",
            category: "training",
            description: "Training session recorded notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: session_created_schema,
        },
        // Visitor templates
        TemplateDefinition {
            id: "visitor.accepted",
            name: "Visitor Accepted",
            category: "transactional",
            description: "Visitor application acceptance notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: visitor_accepted_schema,
        },
        TemplateDefinition {
            id: "visitor.rejected",
            name: "Visitor Rejected",
            category: "transactional",
            description: "Visitor application rejection notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: visitor_rejected_schema,
        },
        // Solo templates
        TemplateDefinition {
            id: "solo.added",
            name: "Solo Granted",
            category: "training",
            description: "Solo certification granted notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: solo_added_schema,
        },
        TemplateDefinition {
            id: "solo.deleted",
            name: "Solo Removed",
            category: "training",
            description: "Solo certification removed notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: solo_deleted_schema,
        },
        TemplateDefinition {
            id: "solo.expired",
            name: "Solo Expired",
            category: "training",
            description: "Solo certification expiration notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: solo_expired_schema,
        },
        // Feedback template
        TemplateDefinition {
            id: "feedback.new",
            name: "New Feedback",
            category: "feedback",
            description: "New feedback received notification",
            is_transactional: false,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: feedback_new_schema,
        },
        // Incident template
        TemplateDefinition {
            id: "incident.closed",
            name: "Incident Closed",
            category: "transactional",
            description: "Incident report closure notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: incident_closed_schema,
        },
        // Broadcast template
        TemplateDefinition {
            id: "broadcast.posted",
            name: "Broadcast Posted",
            category: "announcements",
            description: "Broadcast announcement notification",
            is_transactional: false,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: broadcast_posted_schema,
        },
        // Progression templates
        TemplateDefinition {
            id: "progression.assigned",
            name: "Progression Assigned",
            category: "training",
            description: "Training progression assignment notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: progression_assigned_schema,
        },
        TemplateDefinition {
            id: "progression.removed",
            name: "Progression Removed",
            category: "training",
            description: "Training progression removal notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: progression_removed_schema,
        },
        // Roster template
        TemplateDefinition {
            id: "roster.removed",
            name: "Roster Removed",
            category: "org",
            description: "Controller purged/removed from the roster notification",
            is_transactional: true,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: false,
            payload_schema: roster_removed_schema,
        },
    ]
}

pub fn find_template(template_id: &str) -> Option<&'static TemplateDefinition> {
    registry()
        .iter()
        .find(|template| template.id == template_id)
}

/// Builds a single string property definition with a display `title` and an
/// optional widget `format` hint. Format is used by the website's schema-driven
/// email form to pick a widget: `markdown`, `multiline`, `date`, `date-time`,
/// `uri`, `email` — anything else falls back to a plain text field.
fn prop(title: &str, format: Option<&str>) -> Value {
    match format {
        Some(f) => json!({ "type": "string", "title": title, "format": f }),
        None => json!({ "type": "string", "title": title }),
    }
}

fn announcement_schema() -> Value {
    json!({
        "type": "object",
        "required": ["headline", "body_markdown"],
        "properties": {
            "headline": prop("Headline", None),
            "body_markdown": prop("Body", Some("markdown")),
            "preheader": prop("Preheader", None),
            "cta_label": prop("Call-to-action Label", None),
            "cta_url": prop("Call-to-action URL", Some("uri"))
        }
    })
}

fn event_position_published_schema() -> Value {
    json!({
        "type": "object",
        "required": ["event_title", "starts_at", "details_url"],
        "properties": {
            "event_title": prop("Event Title", None),
            "starts_at": prop("Starts At", Some("date-time")),
            "details_url": prop("Details URL", Some("uri")),
            "preheader": prop("Preheader", None)
        }
    })
}

fn event_reminder_schema() -> Value {
    json!({
        "type": "object",
        "required": ["event_title", "starts_at", "details_url"],
        "properties": {
            "event_title": prop("Event Title", None),
            "starts_at": prop("Starts At", Some("date-time")),
            "details_url": prop("Details URL", Some("uri")),
            "location": prop("Location", None),
            "preheader": prop("Preheader", None)
        }
    })
}

fn system_test_schema() -> Value {
    json!({
        "type": "object",
        "required": ["message"],
        "properties": {
            "message": prop("Message", Some("multiline")),
            "requested_by": prop("Requested By", None)
        }
    })
}

fn loa_approved_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "loa_start", "loa_end"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "loa_start": prop("LOA Start", Some("date")),
            "loa_end": prop("LOA End", Some("date"))
        }
    })
}

fn loa_denied_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "reason": prop("Reason", Some("multiline"))
        }
    })
}

fn loa_deleted_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "reason": prop("Reason", Some("multiline"))
        }
    })
}

fn loa_expired_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name"],
        "properties": {
            "controller_name": prop("Controller Name", None)
        }
    })
}

fn appointment_scheduled_schema() -> Value {
    json!({
        "type": "object",
        "required": ["student_name", "trainer_name", "appointment_start"],
        "properties": {
            "student_name": prop("Student Name", None),
            "trainer_name": prop("Trainer Name", None),
            "appointment_start": prop("Appointment Start", Some("date-time")),
            "details_url": prop("Details URL", Some("uri"))
        }
    })
}

fn appointment_canceled_schema() -> Value {
    json!({
        "type": "object",
        "required": ["student_name", "trainer_name", "appointment_start"],
        "properties": {
            "student_name": prop("Student Name", None),
            "trainer_name": prop("Trainer Name", None),
            "appointment_start": prop("Appointment Start", Some("date-time")),
            "reason": prop("Reason", Some("multiline"))
        }
    })
}

fn appointment_updated_schema() -> Value {
    json!({
        "type": "object",
        "required": ["student_name", "trainer_name", "appointment_start"],
        "properties": {
            "student_name": prop("Student Name", None),
            "trainer_name": prop("Trainer Name", None),
            "appointment_start": prop("Appointment Start", Some("date-time")),
            "details_url": prop("Details URL", Some("uri"))
        }
    })
}

fn appointment_warning_schema() -> Value {
    json!({
        "type": "object",
        "required": ["student_name", "trainer_name", "appointment_start"],
        "properties": {
            "student_name": prop("Student Name", None),
            "trainer_name": prop("Trainer Name", None),
            "appointment_start": prop("Appointment Start", Some("date-time")),
            "warning_message": prop("Warning Message", Some("multiline"))
        }
    })
}

fn session_created_schema() -> Value {
    json!({
        "type": "object",
        "required": ["student_name", "trainer_name", "session_date"],
        "properties": {
            "student_name": prop("Student Name", None),
            "trainer_name": prop("Trainer Name", None),
            "session_date": prop("Session Date", Some("date")),
            "position": prop("Position", None),
            "details_url": prop("Details URL", Some("uri"))
        }
    })
}

fn visitor_accepted_schema() -> Value {
    json!({
        "type": "object",
        "required": ["user_name"],
        "properties": {
            "user_name": prop("User Name", None),
            "artcc_name": prop("ARTCC Name", None),
            "details_url": prop("Details URL", Some("uri"))
        }
    })
}

fn visitor_rejected_schema() -> Value {
    json!({
        "type": "object",
        "required": ["user_name"],
        "properties": {
            "user_name": prop("User Name", None),
            "artcc_name": prop("ARTCC Name", None),
            "reason": prop("Reason", Some("multiline"))
        }
    })
}

fn solo_added_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "position", "expires"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "position": prop("Position", None),
            "expires": prop("Expires", Some("date"))
        }
    })
}

fn solo_deleted_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "position"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "position": prop("Position", None),
            "reason": prop("Reason", Some("multiline"))
        }
    })
}

fn solo_expired_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "position"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "position": prop("Position", None)
        }
    })
}

fn feedback_new_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "position": prop("Position", None),
            "rating": prop("Rating", None),
            "details_url": prop("Details URL", Some("uri"))
        }
    })
}

fn incident_closed_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "incident_date": prop("Incident Date", Some("date")),
            "resolution": prop("Resolution", Some("multiline"))
        }
    })
}

fn broadcast_posted_schema() -> Value {
    json!({
        "type": "object",
        "required": ["title", "body_markdown"],
        "properties": {
            "title": prop("Title", None),
            "body_markdown": prop("Body", Some("markdown")),
            "preheader": prop("Preheader", None),
            "details_url": prop("Details URL", Some("uri"))
        }
    })
}

fn progression_assigned_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "progression_name"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "progression_name": prop("Progression Name", None),
            "details_url": prop("Details URL", Some("uri"))
        }
    })
}

fn progression_removed_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "progression_name"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "progression_name": prop("Progression Name", None),
            "reason": prop("Reason", Some("multiline"))
        }
    })
}

fn roster_removed_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "reason"],
        "properties": {
            "controller_name": prop("Controller Name", None),
            "reason": prop("Reason", Some("multiline"))
        }
    })
}

pub fn unsubscribe_link(
    base_url: Option<&str>,
    secret: Option<&str>,
    category: &str,
    email: &str,
    user_id: Option<&str>,
) -> Option<String> {
    build_unsubscribe_link(base_url?, secret?, category, email, user_id)
}

#[cfg(test)]
mod tests {
    use super::find_template;

    #[test]
    fn registry_contains_expected_templates() {
        let expected = [
            "announcements.generic",
            "events.position_published",
            "events.reminder",
            "system.test_email",
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
            "roster.removed",
        ];

        for id in expected {
            assert!(find_template(id).is_some(), "missing template: {id}");
        }
    }
}

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
    ]
}

pub fn find_template(template_id: &str) -> Option<&'static TemplateDefinition> {
    registry()
        .iter()
        .find(|template| template.id == template_id)
}

fn announcement_schema() -> Value {
    json!({
        "type": "object",
        "required": ["headline", "body_markdown"],
        "properties": {
            "headline": { "type": "string" },
            "body_markdown": { "type": "string" },
            "preheader": { "type": "string" },
            "cta_label": { "type": "string" },
            "cta_url": { "type": "string" }
        }
    })
}

fn event_position_published_schema() -> Value {
    json!({
        "type": "object",
        "required": ["event_title", "starts_at", "details_url"],
        "properties": {
            "event_title": { "type": "string" },
            "starts_at": { "type": "string" },
            "details_url": { "type": "string" },
            "preheader": { "type": "string" }
        }
    })
}

fn event_reminder_schema() -> Value {
    json!({
        "type": "object",
        "required": ["event_title", "starts_at", "details_url"],
        "properties": {
            "event_title": { "type": "string" },
            "starts_at": { "type": "string" },
            "details_url": { "type": "string" },
            "location": { "type": "string" },
            "preheader": { "type": "string" }
        }
    })
}

fn system_test_schema() -> Value {
    json!({
        "type": "object",
        "required": ["message"],
        "properties": {
            "message": { "type": "string" },
            "requested_by": { "type": "string" }
        }
    })
}

fn loa_approved_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "loa_start", "loa_end"],
        "properties": {
            "controller_name": { "type": "string" },
            "loa_start": { "type": "string" },
            "loa_end": { "type": "string" }
        }
    })
}

fn loa_denied_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name"],
        "properties": {
            "controller_name": { "type": "string" },
            "reason": { "type": "string" }
        }
    })
}

fn loa_deleted_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name"],
        "properties": {
            "controller_name": { "type": "string" },
            "reason": { "type": "string" }
        }
    })
}

fn loa_expired_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name"],
        "properties": {
            "controller_name": { "type": "string" }
        }
    })
}

fn appointment_scheduled_schema() -> Value {
    json!({
        "type": "object",
        "required": ["student_name", "trainer_name", "appointment_start"],
        "properties": {
            "student_name": { "type": "string" },
            "trainer_name": { "type": "string" },
            "appointment_start": { "type": "string" },
            "details_url": { "type": "string" }
        }
    })
}

fn appointment_canceled_schema() -> Value {
    json!({
        "type": "object",
        "required": ["student_name", "trainer_name", "appointment_start"],
        "properties": {
            "student_name": { "type": "string" },
            "trainer_name": { "type": "string" },
            "appointment_start": { "type": "string" },
            "reason": { "type": "string" }
        }
    })
}

fn appointment_updated_schema() -> Value {
    json!({
        "type": "object",
        "required": ["student_name", "trainer_name", "appointment_start"],
        "properties": {
            "student_name": { "type": "string" },
            "trainer_name": { "type": "string" },
            "appointment_start": { "type": "string" },
            "details_url": { "type": "string" }
        }
    })
}

fn appointment_warning_schema() -> Value {
    json!({
        "type": "object",
        "required": ["student_name", "trainer_name", "appointment_start"],
        "properties": {
            "student_name": { "type": "string" },
            "trainer_name": { "type": "string" },
            "appointment_start": { "type": "string" },
            "warning_message": { "type": "string" }
        }
    })
}

fn session_created_schema() -> Value {
    json!({
        "type": "object",
        "required": ["student_name", "trainer_name", "session_date"],
        "properties": {
            "student_name": { "type": "string" },
            "trainer_name": { "type": "string" },
            "session_date": { "type": "string" },
            "position": { "type": "string" },
            "details_url": { "type": "string" }
        }
    })
}

fn visitor_accepted_schema() -> Value {
    json!({
        "type": "object",
        "required": ["user_name"],
        "properties": {
            "user_name": { "type": "string" },
            "artcc_name": { "type": "string" },
            "details_url": { "type": "string" }
        }
    })
}

fn visitor_rejected_schema() -> Value {
    json!({
        "type": "object",
        "required": ["user_name"],
        "properties": {
            "user_name": { "type": "string" },
            "artcc_name": { "type": "string" },
            "reason": { "type": "string" }
        }
    })
}

fn solo_added_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "position", "expires"],
        "properties": {
            "controller_name": { "type": "string" },
            "position": { "type": "string" },
            "expires": { "type": "string" }
        }
    })
}

fn solo_deleted_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "position"],
        "properties": {
            "controller_name": { "type": "string" },
            "position": { "type": "string" },
            "reason": { "type": "string" }
        }
    })
}

fn solo_expired_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "position"],
        "properties": {
            "controller_name": { "type": "string" },
            "position": { "type": "string" }
        }
    })
}

fn feedback_new_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name"],
        "properties": {
            "controller_name": { "type": "string" },
            "position": { "type": "string" },
            "rating": { "type": "string" },
            "details_url": { "type": "string" }
        }
    })
}

fn incident_closed_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name"],
        "properties": {
            "controller_name": { "type": "string" },
            "incident_date": { "type": "string" },
            "resolution": { "type": "string" }
        }
    })
}

fn broadcast_posted_schema() -> Value {
    json!({
        "type": "object",
        "required": ["title", "body_markdown"],
        "properties": {
            "title": { "type": "string" },
            "body_markdown": { "type": "string" },
            "preheader": { "type": "string" },
            "details_url": { "type": "string" }
        }
    })
}

fn progression_assigned_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "progression_name"],
        "properties": {
            "controller_name": { "type": "string" },
            "progression_name": { "type": "string" },
            "details_url": { "type": "string" }
        }
    })
}

fn progression_removed_schema() -> Value {
    json!({
        "type": "object",
        "required": ["controller_name", "progression_name"],
        "properties": {
            "controller_name": { "type": "string" },
            "progression_name": { "type": "string" },
            "reason": { "type": "string" }
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
        ];

        for id in expected {
            assert!(find_template(id).is_some(), "missing template: {id}");
        }
    }
}

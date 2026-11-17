//! OTS recommendation, training assignment and trainer release notices. Wording
//! follows the previous vZDC site's emails for the same actions; each notice is a
//! greeting plus plain paragraphs, so they share one renderer.

use maud::html;
use serde_json::Value;

use crate::email::branding::EmailTheme;
use crate::email::rsx::components::EmailLayout;
use crate::email::rsx::text::TextBuilder;
use crate::email::rsx::validate::{optional_string, required_string};
use crate::email::templates::RenderedEmail;
use crate::errors::ApiError;

use super::RsxTemplate;

struct Notice {
    subject: String,
    greeting: String,
    paragraphs: Vec<String>,
}

pub struct TrainingNoticeTemplate {
    id: &'static str,
    build: fn(&Value) -> Result<Notice, ApiError>,
}

impl RsxTemplate for TrainingNoticeTemplate {
    fn id(&self) -> &'static str {
        self.id
    }

    fn render(
        &self,
        payload: &Value,
        theme: &EmailTheme,
        unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError> {
        let notice = (self.build)(payload)?;

        let body = html! {
            p { (notice.greeting) }
            @for paragraph in &notice.paragraphs {
                p { (paragraph) }
            }
        };
        let html = EmailLayout::new(&notice.subject, theme)
            .preheader(&notice.subject)
            .heading(&notice.subject)
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let mut text = TextBuilder::new().line(&notice.greeting);
        for paragraph in &notice.paragraphs {
            text = text.blank().line(paragraph);
        }
        let text = text.optional_unsubscribe(unsubscribe_link).build();

        Ok(RenderedEmail {
            subject: notice.subject,
            html,
            text,
        })
    }
}

const CONTACT_TA: &str = "Contact the Training Administrator if you have any questions.";
const CHECK_PROFILE: &str = "Please check your profile for more details.";
const REACH_OUT: &str = "Feel free to communicate with this student via email or private message.";

fn dear(name: &str) -> String {
    format!("Dear {name},")
}

pub static OTS_ASSIGNED_STUDENT: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.ots_assigned_student",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        let instructor = required_string(payload, "instructor_name")?;
        Ok(Notice {
            subject: "OTS Assignment".to_string(),
            greeting: dear(&student),
            paragraphs: vec![
                format!("{instructor} has been assigned to perform your OTS."),
                "The instructor will reach out to you directly either via email or Discord."
                    .to_string(),
            ],
        })
    },
};

pub static OTS_ASSIGNED_INSTRUCTOR: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.ots_assigned_instructor",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        let instructor = required_string(payload, "instructor_name")?;
        Ok(Notice {
            subject: "OTS Assignment".to_string(),
            greeting: dear(&instructor),
            paragraphs: vec![
                format!("You have been assigned to perform the OTS for {student}."),
                "You may reach out to the trainee by email or using Discord.".to_string(),
            ],
        })
    },
};

pub static OTS_UNASSIGNED_STUDENT: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.ots_unassigned_student",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        Ok(Notice {
            subject: "OTS Assignment Removed".to_string(),
            greeting: dear(&student),
            paragraphs: vec![
                "Your OTS instructor assignment has been removed. Another instructor should be \
                 assigned to you shortly."
                    .to_string(),
                CONTACT_TA.to_string(),
            ],
        })
    },
};

pub static OTS_UNASSIGNED_INSTRUCTOR: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.ots_unassigned_instructor",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        let instructor = required_string(payload, "instructor_name")?;
        Ok(Notice {
            subject: "OTS Assignment Removed".to_string(),
            greeting: dear(&instructor),
            paragraphs: vec![
                format!("Your assignment to perform the OTS for {student} has been removed."),
                CONTACT_TA.to_string(),
            ],
        })
    },
};

pub static OTS_DELETED: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.ots_deleted",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        Ok(Notice {
            subject: "OTS Recommendation Deleted".to_string(),
            greeting: "Hello,".to_string(),
            paragraphs: vec![
                format!("The OTS recommendation for {student} has been deleted."),
                "Contact the Training Administrator for more details.".to_string(),
            ],
        })
    },
};

pub static ASSIGNMENT_CREATED: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.assignment_created",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        let primary = required_string(payload, "primary_trainer_name")?;
        let mut paragraphs = vec![
            "Your trainers have been assigned:".to_string(),
            format!("{primary} (PRIMARY)"),
        ];
        if let Some(others) = optional_string(payload, "other_trainer_names") {
            paragraphs.push(others);
        }
        paragraphs.push(CHECK_PROFILE.to_string());
        Ok(Notice {
            subject: "Training Assignment Fulfilled".to_string(),
            greeting: dear(&student),
            paragraphs,
        })
    },
};

pub static ASSIGNMENT_PRIMARY_TRAINER: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.assignment_primary_trainer",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        let primary = required_string(payload, "primary_trainer_name")?;
        Ok(Notice {
            subject: "Training Assignment - Primary".to_string(),
            greeting: dear(&primary),
            paragraphs: vec![
                format!("You have been assigned as the primary trainer for {student}."),
                "Check the training assignment to view the other trainers for this student."
                    .to_string(),
                REACH_OUT.to_string(),
            ],
        })
    },
};

pub static ASSIGNMENT_OTHER_TRAINER: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.assignment_other_trainer",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        let primary = required_string(payload, "primary_trainer_name")?;
        Ok(Notice {
            subject: "Training Assignment".to_string(),
            greeting: "Dear Trainers,".to_string(),
            paragraphs: vec![
                format!("You have been assigned to train {student} (not primary)."),
                format!("The primary trainer for this student is {primary}."),
                REACH_OUT.to_string(),
            ],
        })
    },
};

pub static ASSIGNMENT_UPDATED: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.assignment_updated",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        Ok(Notice {
            subject: "Training Assignment Updated".to_string(),
            greeting: dear(&student),
            paragraphs: vec![
                "Your training assignment has been updated.".to_string(),
                CHECK_PROFILE.to_string(),
            ],
        })
    },
};

pub static ASSIGNMENT_TRAINER_REMOVED: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.assignment_trainer_removed",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        let cid = required_string(payload, "student_cid")?;
        Ok(Notice {
            subject: "Training Assignment Updated".to_string(),
            greeting: "Dear Trainer(s),".to_string(),
            paragraphs: vec![
                format!(
                    "You have been released from the following training assignment: {student} ({cid})."
                ),
                "You are no longer required to train this student unless they book through the \
                 training scheduler."
                    .to_string(),
            ],
        })
    },
};

pub static ASSIGNMENT_RELEASED: TrainingNoticeTemplate = TrainingNoticeTemplate {
    id: "training.assignment_released",
    build: |payload| {
        let student = required_string(payload, "student_name")?;
        let approved = payload
            .get("release_request_approved")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut paragraphs = Vec::new();
        if approved {
            paragraphs.push("Your release request has been approved.".to_string());
        }
        paragraphs.push(
            "You are no longer part of a training assignment and must use the scheduler for all \
             future training sessions."
                .to_string(),
        );
        paragraphs.push(CHECK_PROFILE.to_string());
        Ok(Notice {
            subject: if approved {
                "Release Request Approved"
            } else {
                "Training Assignment Released"
            }
            .to_string(),
            greeting: dear(&student),
            paragraphs,
        })
    },
};

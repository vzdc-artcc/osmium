//! Notifications for OTS recommendation, training assignment and trainer release
//! changes. Each function queues on the caller's transaction, so the emails commit
//! with the change and its audit row. Recipients are derived from the stored rows
//! before and after the change, never from the request payload.

use std::collections::HashSet;

use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};

use crate::{
    auth::context::CurrentUser,
    email::service::{EmailActor, EmailService},
    errors::ApiError,
    models::{OtsRecommendationSummary, TrainingAssignment},
    repos::{audit as audit_repo, training::assignments as training_assignments_repo},
};

pub(crate) struct TrainingMailer<'a> {
    email: &'a EmailService,
    actor: EmailActor,
}

impl<'a> TrainingMailer<'a> {
    /// The requester recorded on the outbox row and its QUEUE audit row is the
    /// audit actor: the real admin when impersonating, never the impersonated user.
    pub(crate) async fn new(
        email: &'a EmailService,
        tx: &mut Transaction<'_, Postgres>,
        user: &CurrentUser,
    ) -> Result<Self, ApiError> {
        let actor = audit_repo::resolve_audit_actor(&mut **tx, Some(user), None).await?;
        Ok(Self {
            email,
            actor: EmailActor {
                actor_id: actor.actor_id,
                user_id: Some(user.audit_actor_user_id().to_string()),
                service_account_id: None,
                request_source: "api".to_string(),
            },
        })
    }

    async fn send(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        template_id: &str,
        payload: Value,
        user_ids: &[String],
    ) -> Result<(), ApiError> {
        if user_ids.is_empty() {
            return Ok(());
        }
        self.email
            .queue_to_users_in_tx(tx, &self.actor, template_id, payload, user_ids)
            .await?;
        Ok(())
    }

    /// Assigning, replacing or removing the OTS instructor. No change, no email.
    pub(crate) async fn ots_instructor_changed(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        before: &OtsRecommendationSummary,
        after: &OtsRecommendationSummary,
    ) -> Result<(), ApiError> {
        if before.assigned_instructor_id == after.assigned_instructor_id {
            return Ok(());
        }
        let student = &after.student_name;
        let student_ids = [after.student_id.clone()];

        if let (Some(old_id), Some(old_name)) = (
            before.assigned_instructor_id.as_ref(),
            before.assigned_instructor_name.as_ref(),
        ) {
            let payload = json!({ "student_name": student, "instructor_name": old_name });
            self.send(
                tx,
                "training.ots_unassigned_instructor",
                payload,
                &[old_id.clone()],
            )
            .await?;
        }

        match (
            after.assigned_instructor_id.as_ref(),
            after.assigned_instructor_name.as_ref(),
        ) {
            (Some(new_id), Some(new_name)) => {
                let payload = json!({ "student_name": student, "instructor_name": new_name });
                self.send(
                    tx,
                    "training.ots_assigned_student",
                    payload.clone(),
                    &student_ids,
                )
                .await?;
                self.send(
                    tx,
                    "training.ots_assigned_instructor",
                    payload,
                    &[new_id.clone()],
                )
                .await
            }
            _ => {
                let payload = json!({ "student_name": student });
                self.send(tx, "training.ots_unassigned_student", payload, &student_ids)
                    .await
            }
        }
    }

    pub(crate) async fn ots_deleted(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        deleted: &OtsRecommendationSummary,
    ) -> Result<(), ApiError> {
        let mut recipients = vec![deleted.student_id.clone()];
        recipients.extend(deleted.assigned_instructor_id.clone());
        let payload = json!({ "student_name": deleted.student_name });
        self.send(tx, "training.ots_deleted", payload, &recipients)
            .await
    }

    pub(crate) async fn assignment_created(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        row: &TrainingAssignment,
    ) -> Result<(), ApiError> {
        let other_names = row
            .other_trainers
            .iter()
            .map(|trainer| trainer.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let rating =
            training_assignments_repo::fetch_member_rating(&mut **tx, &row.student_id).await?;
        let payload = json!({
            "change": "created",
            "student_name": row.student_name,
            "student_rating": rating,
            "primary_trainer_name": row.primary_trainer_name,
            "other_trainer_names": other_names,
        });
        self.send(
            tx,
            "training.assignment_created",
            payload.clone(),
            &[row.student_id.clone()],
        )
        .await?;
        self.send(
            tx,
            "training.assignment_primary_trainer",
            payload.clone(),
            &[row.primary_trainer_id.clone()],
        )
        .await?;
        self.send(
            tx,
            "training.assignment_other_trainer",
            payload,
            &row.other_trainer_ids,
        )
        .await
    }

    /// Removed trainers are released, newly added other trainers and a new primary
    /// are told of their role, and the student hears that the assignment changed.
    /// A PATCH that leaves the trainer set as it was sends nothing.
    pub(crate) async fn assignment_updated(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        before: &TrainingAssignment,
        after: &TrainingAssignment,
    ) -> Result<(), ApiError> {
        let old_others: HashSet<&String> = before.other_trainer_ids.iter().collect();
        let new_all: HashSet<&String> = after
            .other_trainer_ids
            .iter()
            .chain([&after.primary_trainer_id])
            .collect();
        let removed: Vec<String> = before
            .other_trainer_ids
            .iter()
            .chain([&before.primary_trainer_id])
            .filter(|id| !new_all.contains(id))
            .cloned()
            .collect();
        let added: Vec<String> = after
            .other_trainer_ids
            .iter()
            .filter(|id| !old_others.contains(id))
            .cloned()
            .collect();
        let primary_changed = before.primary_trainer_id != after.primary_trainer_id;
        if removed.is_empty() && added.is_empty() && !primary_changed {
            return Ok(());
        }

        let rating = if primary_changed {
            training_assignments_repo::fetch_member_rating(&mut **tx, &after.student_id).await?
        } else {
            None
        };
        let payload = json!({
            "change": "updated",
            "student_name": after.student_name,
            "student_cid": after.student_cid.to_string(),
            "student_rating": rating,
            "primary_trainer_name": after.primary_trainer_name,
        });
        self.send(
            tx,
            "training.assignment_trainer_removed",
            payload.clone(),
            &removed,
        )
        .await?;
        self.send(
            tx,
            "training.assignment_other_trainer",
            payload.clone(),
            &added,
        )
        .await?;
        if primary_changed {
            self.send(
                tx,
                "training.assignment_primary_trainer",
                payload.clone(),
                &[after.primary_trainer_id.clone()],
            )
            .await?;
        }
        self.send(
            tx,
            "training.assignment_updated",
            payload,
            &[after.student_id.clone()],
        )
        .await
    }

    /// The assignment ended, by deletion or by an approved release request. The
    /// trainers are released only when there was an assignment to end.
    pub(crate) async fn assignment_released(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        student_id: &str,
        student_name: &str,
        student_cid: i64,
        assignment: Option<&TrainingAssignment>,
        release_request_approved: bool,
    ) -> Result<(), ApiError> {
        self.send(
            tx,
            "training.assignment_released",
            json!({
                "student_name": student_name,
                "release_request_approved": release_request_approved,
            }),
            &[student_id.to_string()],
        )
        .await?;
        let Some(assignment) = assignment else {
            return Ok(());
        };
        let mut trainers = vec![assignment.primary_trainer_id.clone()];
        trainers.extend(assignment.other_trainer_ids.iter().cloned());
        self.send(
            tx,
            "training.assignment_trainer_removed",
            json!({
                "change": if release_request_approved { "release_approved" } else { "deleted" },
                "student_name": student_name,
                "student_cid": student_cid.to_string(),
            }),
            &trainers,
        )
        .await
    }
}

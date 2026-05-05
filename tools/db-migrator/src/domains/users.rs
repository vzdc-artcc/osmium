use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

use crate::{
    helpers::{new_id, record_warning},
    mapping::{
        normalize_controller_status, normalize_rating, normalize_role, normalize_staff_position,
    },
    state::AppState,
    target,
};

const DOMAIN: &str = "users";

#[derive(Debug, Clone, FromRow)]
struct SourceUser {
    id: String,
    cid: Option<i64>,
    email: Option<String>,
    email_verified_at: Option<DateTime<Utc>>,
    first_name: Option<String>,
    last_name: Option<String>,
    full_name: String,
    preferred_name: Option<String>,
    joined_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct SourceProfile {
    user_id: String,
    bio: Option<String>,
    timezone: String,
    receive_email: bool,
    new_event_notifications: bool,
    show_welcome_message: bool,
}

#[derive(Debug, Clone, FromRow)]
struct SourceFlags {
    user_id: String,
    no_request_loas: bool,
    no_request_training_assignments: bool,
    no_request_trainer_release: bool,
    no_force_progression_finish: bool,
    no_event_signup: bool,
    no_edit_profile: bool,
    excluded_from_roster_sync: bool,
    hidden_from_roster: bool,
    flag_auto_assign_single_pass: bool,
}

#[derive(Debug, Clone, FromRow)]
struct SourceMembership {
    user_id: String,
    artcc: String,
    division: String,
    rating: Option<String>,
    controller_status: String,
    operating_initials: Option<String>,
    join_date: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct SourceIdentity {
    user_id: String,
    provider: String,
    provider_subject: String,
    provider_username: Option<String>,
    linked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct SourceRoleRow {
    user_id: String,
    role_name: String,
}

#[derive(Debug, Clone, FromRow)]
struct SourceStaffPositionRow {
    user_id: String,
    position_name: String,
}

#[derive(Debug, Clone, Serialize)]
struct UserPayload {
    cid: String,
    email: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    full_name: String,
    preferred_name: Option<String>,
    artcc: String,
    rating: i32,
    division: String,
    operating_initials: Option<String>,
    controller_status: String,
    bio: Option<String>,
    timezone: String,
    receive_email: bool,
    new_event_notifications: bool,
    show_welcome_message: bool,
    no_request_loas: bool,
    no_request_training_assignments: bool,
    no_request_trainer_release: bool,
    no_force_progression_finish: bool,
    no_event_signup: bool,
    no_edit_profile: bool,
    excluded_from_roster_sync: bool,
    hidden_from_roster: bool,
    flag_auto_assign_single_pass: bool,
    join_date: DateTime<Utc>,
    email_verified_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    roles: Vec<String>,
    staff_positions: Vec<String>,
    teamspeak_uid: Option<String>,
    discord_uid: Option<String>,
    discord_tag: Option<String>,
    discord_connected_at: Option<DateTime<Utc>>,
}

pub async fn migrate(state: &mut AppState) -> Result<()> {
    let users = sqlx::query_as::<_, SourceUser>(
        r#"
        select id, cid, email::text as email, email_verified_at, first_name, last_name, full_name, preferred_name, joined_at, updated_at
        from identity.users
        order by created_at asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let profiles = sqlx::query_as::<_, SourceProfile>(
        r#"
        select user_id, bio, timezone, receive_email, new_event_notifications, show_welcome_message
        from identity.user_profiles
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let flags = sqlx::query_as::<_, SourceFlags>(
        r#"
        select user_id, no_request_loas, no_request_training_assignments, no_request_trainer_release,
               no_force_progression_finish, no_event_signup, no_edit_profile, excluded_from_roster_sync,
               hidden_from_roster, flag_auto_assign_single_pass
        from identity.user_flags
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let memberships = sqlx::query_as::<_, SourceMembership>(
        r#"
        select user_id, artcc, division, rating, controller_status, operating_initials, join_date
        from org.memberships
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let identities = sqlx::query_as::<_, SourceIdentity>(
        r#"
        select user_id, provider, provider_subject, provider_username, linked_at
        from identity.user_identities
        where provider in ('DISCORD', 'TEAMSPEAK')
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let roles =
        sqlx::query_as::<_, SourceRoleRow>(r#"select user_id, role_name from access.user_roles"#)
            .fetch_all(&state.source)
            .await?;
    let staff_positions = sqlx::query_as::<_, SourceStaffPositionRow>(
        r#"
        select usp.user_id, sp.name as position_name
        from org.user_staff_positions usp
        join org.staff_positions sp on sp.id = usp.staff_position_id
        where usp.ends_at is null or usp.ends_at > now()
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    let profile_by_user = profiles
        .into_iter()
        .map(|row| (row.user_id.clone(), row))
        .collect::<HashMap<_, _>>();
    let flags_by_user = flags
        .into_iter()
        .map(|row| (row.user_id.clone(), row))
        .collect::<HashMap<_, _>>();
    let membership_by_user = memberships
        .into_iter()
        .map(|row| (row.user_id.clone(), row))
        .collect::<HashMap<_, _>>();

    let mut identities_by_user: HashMap<String, Vec<SourceIdentity>> = HashMap::new();
    for row in identities {
        identities_by_user
            .entry(row.user_id.clone())
            .or_default()
            .push(row);
    }

    let mut roles_by_user: HashMap<String, Vec<String>> = HashMap::new();
    for row in roles {
        roles_by_user
            .entry(row.user_id)
            .or_default()
            .push(row.role_name);
    }

    let mut staff_by_user: HashMap<String, Vec<String>> = HashMap::new();
    for row in staff_positions {
        staff_by_user
            .entry(row.user_id)
            .or_default()
            .push(row.position_name);
    }

    for user in users {
        let domain = state.report.domain_mut(DOMAIN);
        domain.planned += 1;

        let cid = user.cid.context("source user missing cid")?.to_string();
        let membership = membership_by_user.get(&user.id);
        let profile = profile_by_user.get(&user.id);
        let flags = flags_by_user.get(&user.id);
        let linked = identities_by_user.get(&user.id);

        let rating = match membership.and_then(|row| row.rating.as_deref()) {
            Some(value) => normalize_rating(value)?,
            None => {
                record_warning(
                    state,
                    DOMAIN,
                    "user",
                    &user.id,
                    "membership rating missing; defaulting to OBS",
                )
                .await?;
                1
            }
        };

        let controller_status = normalize_controller_status(
            membership
                .map(|row| row.controller_status.as_str())
                .unwrap_or("NONE"),
        )?
        .to_string();

        let mut target_roles = BTreeSet::new();
        for role_name in roles_by_user.get(&user.id).cloned().unwrap_or_default() {
            if let Some(mapped) = normalize_role(&role_name) {
                target_roles.insert(mapped.to_string());
                continue;
            }

            if normalize_staff_position(&role_name).is_some()
                || matches!(role_name.as_str(), "BOT" | "SERVICE_APP")
            {
                continue;
            }

            if state.config.strict {
                bail!("unknown user role `{role_name}` for user {}", user.id);
            }
            record_warning(
                state,
                DOMAIN,
                "user-role",
                &user.id,
                format!("skipping unmapped role `{role_name}`"),
            )
            .await?;
        }
        if target_roles.is_empty() {
            target_roles.insert("CONTROLLER".to_string());
        }

        let mut target_staff_positions = BTreeSet::new();
        for position_name in staff_by_user.get(&user.id).cloned().unwrap_or_default() {
            let mapped = normalize_staff_position(&position_name).with_context(|| {
                format!(
                    "unknown staff position `{position_name}` for user {}",
                    user.id
                )
            })?;
            target_staff_positions.insert(mapped.to_string());
        }

        let mut teamspeak_uid = None;
        let mut discord_uid = None;
        let mut discord_tag = None;
        let mut discord_connected_at = None;
        if let Some(items) = linked {
            for identity in items {
                match identity.provider.as_str() {
                    "TEAMSPEAK" => teamspeak_uid = Some(identity.provider_subject.clone()),
                    "DISCORD" => {
                        discord_uid = Some(identity.provider_subject.clone());
                        discord_tag = identity.provider_username.clone();
                        discord_connected_at = Some(identity.linked_at);
                    }
                    _ => {}
                }
            }
        }

        let payload = UserPayload {
            cid: cid.clone(),
            email: user.email.clone(),
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            full_name: user.full_name.clone(),
            preferred_name: user.preferred_name.clone(),
            artcc: membership
                .map(|row| row.artcc.clone())
                .unwrap_or_else(|| "ZDC".to_string()),
            rating,
            division: membership
                .map(|row| row.division.clone())
                .unwrap_or_else(|| "USA".to_string()),
            operating_initials: membership.and_then(|row| row.operating_initials.clone()),
            controller_status,
            bio: profile.and_then(|row| row.bio.clone()),
            timezone: profile
                .map(|row| row.timezone.clone())
                .unwrap_or_else(|| "America/New_York".to_string()),
            receive_email: profile.map(|row| row.receive_email).unwrap_or(true),
            new_event_notifications: profile
                .map(|row| row.new_event_notifications)
                .unwrap_or(false),
            show_welcome_message: profile.map(|row| row.show_welcome_message).unwrap_or(false),
            no_request_loas: flags.map(|row| row.no_request_loas).unwrap_or(false),
            no_request_training_assignments: flags
                .map(|row| row.no_request_training_assignments)
                .unwrap_or(false),
            no_request_trainer_release: flags
                .map(|row| row.no_request_trainer_release)
                .unwrap_or(false),
            no_force_progression_finish: flags
                .map(|row| row.no_force_progression_finish)
                .unwrap_or(false),
            no_event_signup: flags.map(|row| row.no_event_signup).unwrap_or(false),
            no_edit_profile: flags.map(|row| row.no_edit_profile).unwrap_or(false),
            excluded_from_roster_sync: flags
                .map(|row| row.excluded_from_roster_sync)
                .unwrap_or(false),
            hidden_from_roster: flags.map(|row| row.hidden_from_roster).unwrap_or(false),
            flag_auto_assign_single_pass: flags
                .map(|row| row.flag_auto_assign_single_pass)
                .unwrap_or(false),
            join_date: membership
                .map(|row| row.join_date)
                .unwrap_or(user.joined_at),
            email_verified_at: user.email_verified_at,
            updated_at: user.updated_at,
            roles: target_roles.into_iter().collect(),
            staff_positions: target_staff_positions.into_iter().collect(),
            teamspeak_uid,
            discord_uid,
            discord_tag,
            discord_connected_at,
        };

        upsert_user(state, &user.id, &payload).await?;
    }

    if !state.config.dry_run {
        target::checkpoint(&state.target, &state.config.run_id, DOMAIN, "users").await?;
    }
    Ok(())
}

async fn upsert_user(state: &mut AppState, source_id: &str, payload: &UserPayload) -> Result<()> {
    let source_business_key = format!("cid:{}", payload.cid);
    let mut target_id =
        if let Some(mapping) = target::find_mapping(&state.target, "user", source_id).await? {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(r#"select id from "User" where "cid" = $1"#)
                .bind(&payload.cid)
                .fetch_optional(&state.target)
                .await?
        };

    let existed = target_id.is_some();
    if target_id.is_none() {
        target_id = Some(new_id());
    }
    let target_id = target_id.expect("target id exists");

    if !state.config.dry_run {
        sqlx::query(
            r#"
            insert into "User" (
                "id", "cid", "firstName", "lastName", "fullName", "email", "emailVerified",
                "artcc", "rating", "division", "roles", "staffPositions", "preferredName",
                "bio", "controllerStatus", "updatedAt", "joinDate", "discordUid",
                "discordConnectedAt", "discordTag", "timezone", "showWelcomeMessage",
                "teamspeakUid", "newEventNotifications", "receiveEmail", "noRequestLoas",
                "noRequestTrainingAssignments", "noRequestTrainerRelease", "noForceProgressionFinish",
                "noEventSignup", "noEditProfile", "excludedFromVatusaRosterUpdate",
                "hiddenFromRoster", "operatingInitials", "flagAutoAssignSinglePass"
            )
            values (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::"Role"[], $12::"StaffPosition"[],
                $13, $14, $15::"ControllerStatus", $16, $17, $18, $19, $20, $21, $22, $23, $24,
                $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, $35
            )
            on conflict ("id") do update set
                "cid" = excluded."cid",
                "firstName" = excluded."firstName",
                "lastName" = excluded."lastName",
                "fullName" = excluded."fullName",
                "email" = excluded."email",
                "emailVerified" = excluded."emailVerified",
                "artcc" = excluded."artcc",
                "rating" = excluded."rating",
                "division" = excluded."division",
                "roles" = excluded."roles",
                "staffPositions" = excluded."staffPositions",
                "preferredName" = excluded."preferredName",
                "bio" = excluded."bio",
                "controllerStatus" = excluded."controllerStatus",
                "updatedAt" = excluded."updatedAt",
                "joinDate" = excluded."joinDate",
                "discordUid" = excluded."discordUid",
                "discordConnectedAt" = excluded."discordConnectedAt",
                "discordTag" = excluded."discordTag",
                "timezone" = excluded."timezone",
                "showWelcomeMessage" = excluded."showWelcomeMessage",
                "teamspeakUid" = excluded."teamspeakUid",
                "newEventNotifications" = excluded."newEventNotifications",
                "receiveEmail" = excluded."receiveEmail",
                "noRequestLoas" = excluded."noRequestLoas",
                "noRequestTrainingAssignments" = excluded."noRequestTrainingAssignments",
                "noRequestTrainerRelease" = excluded."noRequestTrainerRelease",
                "noForceProgressionFinish" = excluded."noForceProgressionFinish",
                "noEventSignup" = excluded."noEventSignup",
                "noEditProfile" = excluded."noEditProfile",
                "excludedFromVatusaRosterUpdate" = excluded."excludedFromVatusaRosterUpdate",
                "hiddenFromRoster" = excluded."hiddenFromRoster",
                "operatingInitials" = excluded."operatingInitials",
                "flagAutoAssignSinglePass" = excluded."flagAutoAssignSinglePass"
            "#,
        )
        .bind(&target_id)
        .bind(&payload.cid)
        .bind(&payload.first_name)
        .bind(&payload.last_name)
        .bind(&payload.full_name)
        .bind(&payload.email)
        .bind(payload.email_verified_at)
        .bind(&payload.artcc)
        .bind(payload.rating)
        .bind(&payload.division)
        .bind(&payload.roles)
        .bind(&payload.staff_positions)
        .bind(&payload.preferred_name)
        .bind(&payload.bio)
        .bind(&payload.controller_status)
        .bind(payload.updated_at)
        .bind(payload.join_date)
        .bind(&payload.discord_uid)
        .bind(payload.discord_connected_at)
        .bind(&payload.discord_tag)
        .bind(&payload.timezone)
        .bind(payload.show_welcome_message)
        .bind(&payload.teamspeak_uid)
        .bind(payload.new_event_notifications)
        .bind(payload.receive_email)
        .bind(payload.no_request_loas)
        .bind(payload.no_request_training_assignments)
        .bind(payload.no_request_trainer_release)
        .bind(payload.no_force_progression_finish)
        .bind(payload.no_event_signup)
        .bind(payload.no_edit_profile)
        .bind(payload.excluded_from_roster_sync)
        .bind(payload.hidden_from_roster)
        .bind(&payload.operating_initials)
        .bind(payload.flag_auto_assign_single_pass)
        .execute(&state.target)
        .await?;

        target::upsert_mapping(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "user",
            source_id,
            &source_business_key,
            &target_id,
            &source_business_key,
            if existed { "updated" } else { "created" },
            payload,
        )
        .await?;
    }

    let domain = state.report.domain_mut(DOMAIN);
    if existed {
        domain.updated += 1;
    } else {
        domain.created += 1;
    }
    Ok(())
}

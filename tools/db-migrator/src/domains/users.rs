use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

use crate::{
    helpers::{assume_utc, assume_utc_opt, record_warning},
    mapping::{
        legacy_numeric_rating_to_code, normalize_controller_status, normalize_role,
        normalize_staff_position,
    },
    state::AppState,
    target,
};

const DOMAIN: &str = "users";

#[derive(Debug, Clone, FromRow)]
struct SourceUser {
    id: String,
    cid: Option<String>,
    email: Option<String>,
    email_verified_at: Option<NaiveDateTime>,
    first_name: Option<String>,
    last_name: Option<String>,
    full_name: String,
    preferred_name: Option<String>,
    artcc: Option<String>,
    rating: Option<i32>,
    division: Option<String>,
    roles: Vec<String>,
    staff_positions: Vec<String>,
    bio: Option<String>,
    controller_status: Option<String>,
    updated_at: NaiveDateTime,
    no_request_loas: bool,
    no_event_signup: bool,
    no_edit_profile: bool,
    excluded_from_roster_sync: bool,
    hidden_from_roster: bool,
    operating_initials: Option<String>,
    no_request_trainer_release: bool,
    no_request_training_assignments: bool,
    receive_email: bool,
    flag_auto_assign_single_pass: bool,
    no_force_progression_finish: bool,
    new_event_notifications: bool,
    teamspeak_uid: Option<String>,
    discord_uid: Option<String>,
    join_date: NaiveDateTime,
    timezone: Option<String>,
    show_welcome_message: bool,
    discord_connected_at: Option<NaiveDateTime>,
    discord_tag: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct UserPayload {
    cid: i64,
    email: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    full_name: String,
    preferred_name: Option<String>,
    display_name: String,
    artcc: String,
    division: String,
    rating: String,
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
    operating_initials: Option<String>,
    join_date: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    email_verified_at: Option<DateTime<Utc>>,
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
        select
            id,
            cid,
            email,
            "emailVerified" as email_verified_at,
            "firstName" as first_name,
            "lastName" as last_name,
            "fullName" as full_name,
            "preferredName" as preferred_name,
            artcc,
            rating,
            division,
            coalesce(roles::text[], '{}'::text[]) as roles,
            coalesce("staffPositions"::text[], '{}'::text[]) as staff_positions,
            bio,
            "controllerStatus"::text as controller_status,
            "updatedAt" as updated_at,
            coalesce("noRequestLoas", false) as no_request_loas,
            coalesce("noEventSignup", false) as no_event_signup,
            coalesce("noEditProfile", false) as no_edit_profile,
            coalesce("excludedFromVatusaRosterUpdate", false) as excluded_from_roster_sync,
            coalesce("hiddenFromRoster", false) as hidden_from_roster,
            "operatingInitials" as operating_initials,
            coalesce("noRequestTrainerRelease", false) as no_request_trainer_release,
            coalesce("noRequestTrainingAssignments", false) as no_request_training_assignments,
            coalesce("receiveEmail", true) as receive_email,
            "flagAutoAssignSinglePass" as flag_auto_assign_single_pass,
            coalesce("noForceProgressionFinish", false) as no_force_progression_finish,
            coalesce("newEventNotifications", false) as new_event_notifications,
            "teamspeakUid" as teamspeak_uid,
            "discordUid" as discord_uid,
            "joinDate" as join_date,
            timezone,
            "showWelcomeMessage" as show_welcome_message,
            "discordConnectedAt" as discord_connected_at,
            "discordTag" as discord_tag
        from public."User"
        order by "joinDate" asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    let staff_position_lookup =
        sqlx::query_as::<_, TargetStaffPosition>(r#"select id, name from org.staff_positions"#)
            .fetch_all(&state.target)
            .await?
            .into_iter()
            .map(|row| (row.name, row.id))
            .collect::<HashMap<_, _>>();
    let duplicate_operating_initials = users
        .iter()
        .filter_map(|user| {
            user.operating_initials
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .fold(HashMap::<String, usize>::new(), |mut acc, initials| {
            *acc.entry(initials).or_insert(0) += 1;
            acc
        })
        .into_iter()
        .filter_map(|(initials, count)| (count > 1).then_some(initials))
        .collect::<HashSet<_>>();

    for user in users {
        state.report.domain_mut(DOMAIN).planned += 1;
        let payload = build_payload(state, &user, &duplicate_operating_initials).await?;
        upsert_user(state, &user.id, &payload, &staff_position_lookup).await?;
    }

    if !state.config.dry_run {
        target::checkpoint(&state.target, &state.config.run_id, DOMAIN, "users").await?;
    }
    Ok(())
}

#[derive(Debug, Clone, FromRow)]
struct TargetStaffPosition {
    id: String,
    name: String,
}

async fn build_payload(
    state: &mut AppState,
    user: &SourceUser,
    duplicate_operating_initials: &HashSet<String>,
) -> Result<UserPayload> {
    let cid = user
        .cid
        .as_deref()
        .context("legacy user missing cid")?
        .parse::<i64>()
        .with_context(|| {
            format!(
                "invalid legacy cid `{}` for user {}",
                user.cid.clone().unwrap_or_default(),
                user.id
            )
        })?;

    let controller_status =
        normalize_controller_status(user.controller_status.as_deref().unwrap_or("NONE"))?
            .to_string();

    let rating = legacy_numeric_rating_to_code(user.rating.unwrap_or(1)).to_string();
    if rating == "SUS" {
        record_warning(
            state,
            DOMAIN,
            "user",
            &user.id,
            format!(
                "unknown numeric rating {:?}; defaulting to SUS",
                user.rating.unwrap_or_default()
            ),
        )
        .await?;
    }

    let mut roles = BTreeSet::new();
    for role_name in &user.roles {
        if let Some(mapped) = normalize_role(role_name) {
            roles.insert(mapped.to_string());
            continue;
        }

        if normalize_staff_position(role_name).is_some()
            || matches!(role_name.as_str(), "BOT" | "SERVICE_APP")
        {
            continue;
        }

        if state.config.strict {
            bail!("unknown legacy role `{role_name}` for user {}", user.id);
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
    if roles.is_empty() {
        roles.insert("USER".to_string());
    }

    let mut staff_positions = BTreeSet::new();
    for position_name in &user.staff_positions {
        let mapped = normalize_staff_position(position_name).with_context(|| {
            format!(
                "unknown legacy staff position `{position_name}` for user {}",
                user.id
            )
        })?;
        staff_positions.insert(mapped.to_string());
    }

    let display_name = user
        .preferred_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(user.full_name.as_str())
        .to_string();
    let operating_initials = user
        .operating_initials
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let operating_initials = if let Some(initials) = operating_initials {
        if duplicate_operating_initials.contains(&initials) {
            record_warning(
                state,
                DOMAIN,
                "user-membership",
                &user.id,
                format!("dropping duplicate operating initials `{initials}`"),
            )
            .await?;
            None
        } else {
            Some(initials)
        }
    } else {
        None
    };

    Ok(UserPayload {
        cid,
        email: user.email.clone(),
        first_name: user.first_name.clone(),
        last_name: user.last_name.clone(),
        full_name: user.full_name.clone(),
        preferred_name: user.preferred_name.clone(),
        display_name,
        artcc: user.artcc.clone().unwrap_or_else(|| "ZDC".to_string()),
        division: user.division.clone().unwrap_or_else(|| "USA".to_string()),
        rating,
        controller_status,
        bio: user.bio.clone(),
        timezone: user
            .timezone
            .clone()
            .unwrap_or_else(|| "America/New_York".to_string()),
        receive_email: user.receive_email,
        new_event_notifications: user.new_event_notifications,
        show_welcome_message: user.show_welcome_message,
        no_request_loas: user.no_request_loas,
        no_request_training_assignments: user.no_request_training_assignments,
        no_request_trainer_release: user.no_request_trainer_release,
        no_force_progression_finish: user.no_force_progression_finish,
        no_event_signup: user.no_event_signup,
        no_edit_profile: user.no_edit_profile,
        excluded_from_roster_sync: user.excluded_from_roster_sync,
        hidden_from_roster: user.hidden_from_roster,
        flag_auto_assign_single_pass: user.flag_auto_assign_single_pass,
        operating_initials,
        join_date: assume_utc(user.join_date),
        updated_at: assume_utc(user.updated_at),
        email_verified_at: assume_utc_opt(user.email_verified_at),
        roles: roles.into_iter().collect(),
        staff_positions: staff_positions.into_iter().collect(),
        teamspeak_uid: user.teamspeak_uid.clone(),
        discord_uid: user.discord_uid.clone(),
        discord_tag: user.discord_tag.clone(),
        discord_connected_at: assume_utc_opt(user.discord_connected_at),
    })
}

async fn upsert_user(
    state: &mut AppState,
    source_id: &str,
    payload: &UserPayload,
    staff_position_lookup: &HashMap<String, String>,
) -> Result<()> {
    let source_business_key = format!("cid:{}", payload.cid);
    let target_id =
        if let Some(mapping) = target::find_mapping(&state.target, "user", source_id).await? {
            mapping.target_id
        } else if let Some(existing_id) =
            sqlx::query_scalar::<_, String>(r#"select id from identity.users where cid = $1"#)
                .bind(payload.cid)
                .fetch_optional(&state.target)
                .await?
        {
            existing_id
        } else {
            source_id.to_string()
        };
    let existed = sqlx::query_scalar::<_, bool>(
        r#"select exists(select 1 from identity.users where id = $1)"#,
    )
    .bind(&target_id)
    .fetch_one(&state.target)
    .await?;

    if !state.config.dry_run {
        sqlx::query(
            r#"
            insert into identity.users (
                id, cid, email, email_verified_at, first_name, last_name, full_name,
                preferred_name, display_name, joined_at, updated_at
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            on conflict (id) do update set
                cid = excluded.cid,
                email = excluded.email,
                email_verified_at = excluded.email_verified_at,
                first_name = excluded.first_name,
                last_name = excluded.last_name,
                full_name = excluded.full_name,
                preferred_name = excluded.preferred_name,
                display_name = excluded.display_name,
                joined_at = excluded.joined_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&target_id)
        .bind(payload.cid)
        .bind(&payload.email)
        .bind(payload.email_verified_at)
        .bind(&payload.first_name)
        .bind(&payload.last_name)
        .bind(&payload.full_name)
        .bind(&payload.preferred_name)
        .bind(&payload.display_name)
        .bind(payload.join_date)
        .bind(payload.updated_at)
        .execute(&state.target)
        .await?;

        sqlx::query(
            r#"
            insert into identity.user_profiles (
                user_id, bio, timezone, receive_email, new_event_notifications, show_welcome_message
            )
            values ($1, $2, $3, $4, $5, $6)
            on conflict (user_id) do update set
                bio = excluded.bio,
                timezone = excluded.timezone,
                receive_email = excluded.receive_email,
                new_event_notifications = excluded.new_event_notifications,
                show_welcome_message = excluded.show_welcome_message
            "#,
        )
        .bind(&target_id)
        .bind(&payload.bio)
        .bind(&payload.timezone)
        .bind(payload.receive_email)
        .bind(payload.new_event_notifications)
        .bind(payload.show_welcome_message)
        .execute(&state.target)
        .await?;

        sqlx::query(
            r#"
            insert into identity.user_flags (
                user_id, no_request_loas, no_request_training_assignments, no_request_trainer_release,
                no_force_progression_finish, no_event_signup, no_edit_profile, excluded_from_roster_sync,
                hidden_from_roster, flag_auto_assign_single_pass
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            on conflict (user_id) do update set
                no_request_loas = excluded.no_request_loas,
                no_request_training_assignments = excluded.no_request_training_assignments,
                no_request_trainer_release = excluded.no_request_trainer_release,
                no_force_progression_finish = excluded.no_force_progression_finish,
                no_event_signup = excluded.no_event_signup,
                no_edit_profile = excluded.no_edit_profile,
                excluded_from_roster_sync = excluded.excluded_from_roster_sync,
                hidden_from_roster = excluded.hidden_from_roster,
                flag_auto_assign_single_pass = excluded.flag_auto_assign_single_pass
            "#,
        )
        .bind(&target_id)
        .bind(payload.no_request_loas)
        .bind(payload.no_request_training_assignments)
        .bind(payload.no_request_trainer_release)
        .bind(payload.no_force_progression_finish)
        .bind(payload.no_event_signup)
        .bind(payload.no_edit_profile)
        .bind(payload.excluded_from_roster_sync)
        .bind(payload.hidden_from_roster)
        .bind(payload.flag_auto_assign_single_pass)
        .execute(&state.target)
        .await?;

        sqlx::query(
            r#"
            insert into org.memberships (
                user_id, artcc, division, rating, controller_status, operating_initials, join_date
            )
            values ($1, $2, $3, $4, $5, $6, $7)
            on conflict (user_id) do update set
                artcc = excluded.artcc,
                division = excluded.division,
                rating = excluded.rating,
                controller_status = excluded.controller_status,
                operating_initials = excluded.operating_initials,
                join_date = excluded.join_date
            "#,
        )
        .bind(&target_id)
        .bind(&payload.artcc)
        .bind(&payload.division)
        .bind(&payload.rating)
        .bind(&payload.controller_status)
        .bind(&payload.operating_initials)
        .bind(payload.join_date)
        .execute(&state.target)
        .await?;

        sqlx::query(r#"delete from access.user_roles where user_id = $1"#)
            .bind(&target_id)
            .execute(&state.target)
            .await?;
        for role_name in &payload.roles {
            sqlx::query(
                r#"insert into access.user_roles (user_id, role_name) values ($1, $2) on conflict do nothing"#,
            )
            .bind(&target_id)
            .bind(role_name)
            .execute(&state.target)
            .await?;
        }

        sqlx::query(
            r#"delete from org.user_staff_positions where user_id = $1 and ends_at is null"#,
        )
        .bind(&target_id)
        .execute(&state.target)
        .await?;
        for position_name in &payload.staff_positions {
            let staff_position_id = staff_position_lookup
                .get(position_name)
                .with_context(|| format!("missing target staff position `{position_name}`"))?;
            sqlx::query(
                r#"
                insert into org.user_staff_positions (id, user_id, staff_position_id, starts_at)
                values (gen_random_uuid()::text, $1, $2, $3)
                "#,
            )
            .bind(&target_id)
            .bind(staff_position_id)
            .bind(payload.join_date)
            .execute(&state.target)
            .await?;
        }

        if let Some(teamspeak_uid) = payload.teamspeak_uid.as_deref() {
            upsert_identity(
                &state.target,
                &target_id,
                "TEAMSPEAK",
                teamspeak_uid,
                None,
                payload.join_date,
            )
            .await?;
        }
        if let Some(discord_uid) = payload.discord_uid.as_deref() {
            upsert_identity(
                &state.target,
                &target_id,
                "DISCORD",
                discord_uid,
                payload.discord_tag.as_deref(),
                payload.discord_connected_at.unwrap_or(payload.join_date),
            )
            .await?;
        }

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

async fn upsert_identity(
    pool: &sqlx::PgPool,
    user_id: &str,
    provider: &str,
    provider_subject: &str,
    provider_username: Option<&str>,
    linked_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        r#"
        insert into identity.user_identities (
            user_id, provider, provider_subject, provider_username, linked_at
        )
        values ($1, $2, $3, $4, $5)
        on conflict (provider, provider_subject) do update set
            user_id = excluded.user_id,
            provider_username = excluded.provider_username,
            linked_at = excluded.linked_at
        "#,
    )
    .bind(user_id)
    .bind(provider)
    .bind(provider_subject)
    .bind(provider_username)
    .bind(linked_at)
    .execute(pool)
    .await?;
    Ok(())
}

use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    auth::{
        acl::{PermissionPath, SERVER_ADMIN_ROLE},
        context::{CurrentServiceAccount, CurrentUser},
    },
    errors::ApiError,
};

const DEFAULT_TIMEZONE: &str = "America/New_York";

pub async fn find_current_user_by_session_token(
    pool: &PgPool,
    session_token: &str,
) -> Result<Option<CurrentUser>, ApiError> {
    sqlx::query_as::<_, CurrentUser>(
        r#"
        select
            u.id,
            u.cid,
            coalesce(u.email::text, '') as email,
            u.display_name,
            coalesce(p.timezone, $2) as timezone,
            m.rating,
            pr.primary_role,
            s.impersonator_user_id,
            imp.cid as impersonator_cid,
            imp.display_name as impersonator_display_name
        from identity.sessions s
        join identity.users u on u.id = s.user_id
        left join identity.user_profiles p on p.user_id = u.id
        left join org.memberships m on m.user_id = u.id
        left join access.v_user_primary_role pr on pr.user_id = u.id
        left join identity.users imp on imp.id = s.impersonator_user_id
        where s.session_token = $1
          and s.revoked_at is null
          and s.expires_at > now()
        "#,
    )
    .bind(session_token)
    .bind(DEFAULT_TIMEZONE)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn find_current_user_by_cid(
    pool: &PgPool,
    cid: i64,
) -> Result<Option<CurrentUser>, ApiError> {
    sqlx::query_as::<_, CurrentUser>(
        r#"
        select
            u.id,
            u.cid,
            coalesce(u.email::text, '') as email,
            u.display_name,
            coalesce(p.timezone, $2) as timezone,
            m.rating,
            pr.primary_role
        from identity.users u
        left join identity.user_profiles p on p.user_id = u.id
        left join org.memberships m on m.user_id = u.id
        left join access.v_user_primary_role pr on pr.user_id = u.id
        where u.cid = $1
        "#,
    )
    .bind(cid)
    .bind(DEFAULT_TIMEZONE)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn find_current_service_account_by_bearer_token(
    pool: &PgPool,
    bearer_token: &str,
) -> Result<Option<CurrentServiceAccount>, ApiError> {
    let token_hash = sha256_hex(bearer_token);

    let account = sqlx::query_as::<_, CurrentServiceAccount>(
        r#"
        select
            sa.id,
            sa.key,
            sa.name
        from access.service_account_credentials sac
        join access.service_accounts sa on sa.id = sac.service_account_id
        where sac.secret_hash = $1
          and sac.revoked_at is null
          and (sac.expires_at is null or sac.expires_at > now())
          and sa.status = 'active'
        order by sac.created_at desc
        limit 1
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    if let Some(account) = account {
        sqlx::query(
            r#"
            update access.service_account_credentials
            set last_used_at = now()
            where service_account_id = $1
              and secret_hash = $2
            "#,
        )
        .bind(&account.id)
        .bind(token_hash)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

        Ok(Some(account))
    } else {
        Ok(None)
    }
}

pub async fn fetch_user_role_names(pool: &PgPool, user_id: &str) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        "select role_name from access.user_roles where user_id = $1 order by role_name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_user_permission_names(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        r#"
        select permission_name
        from access.v_effective_user_permissions
        where user_id = $1
        order by permission_name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Direct grants only (`access.user_permissions`), unmerged with role
/// permissions — unlike `fetch_user_permission_names`, which reads the
/// effective view. Needed to diff a permission-editor save against what
/// actually changed as a *direct* grant, rather than the effective set
/// (which also includes role-derived permissions that were never a row
/// here and shouldn't count as "added" just because a save re-submitted
/// them unchanged).
pub async fn fetch_user_direct_permission_names(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        r#"
        select permission_name
        from access.user_permissions
        where user_id = $1
          and granted = true
        order by permission_name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_service_account_role_names(
    pool: &PgPool,
    service_account_id: &str,
) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        r#"
        select role_name
        from access.service_account_roles
        where service_account_id = $1
          and (ends_at is null or ends_at > now())
        order by role_name
        "#,
    )
    .bind(service_account_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_service_account_permission_names(
    pool: &PgPool,
    service_account_id: &str,
) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        r#"
        select permission_name
        from access.v_effective_service_account_permissions
        where service_account_id = $1
        order by permission_name
        "#,
    )
    .bind(service_account_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_access_catalog_names(
    pool: &PgPool,
) -> Result<(Vec<String>, Vec<String>), ApiError> {
    let roles = sqlx::query_scalar::<_, String>("select name from access.roles order by name")
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    let permissions =
        sqlx::query_scalar::<_, String>("select name from access.permissions order by name")
            .fetch_all(pool)
            .await
            .map_err(|_| ApiError::Internal)?;

    Ok((roles, permissions))
}

pub async fn find_user_id_by_cid(pool: &PgPool, cid: i64) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>("select id from identity.users where cid = $1")
        .bind(cid)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn replace_user_permissions(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    permissions: &[String],
) -> Result<(), ApiError> {
    sqlx::query("delete from access.user_permissions where user_id = $1")
        .bind(user_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

    for permission_name in permissions {
        sqlx::query(
            r#"
            insert into access.user_permissions (user_id, permission_name, granted)
            values ($1, $2, true)
            on conflict (user_id, permission_name) do update
            set granted = true
            "#,
        )
        .bind(user_id)
        .bind(permission_name)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    }

    Ok(())
}

pub async fn assign_server_admin(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> Result<(), ApiError> {
    sqlx::query("delete from access.user_roles where role_name = $1")
        .bind(SERVER_ADMIN_ROLE)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

    sqlx::query("delete from access.user_roles where user_id = $1")
        .bind(user_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

    sqlx::query("delete from access.user_permissions where user_id = $1")
        .bind(user_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into access.user_roles (user_id, role_name)
        values ($1, $2)
        on conflict (user_id, role_name) do nothing
        "#,
    )
    .bind(user_id)
    .bind(SERVER_ADMIN_ROLE)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

/// VATUSA facility roles that auto-sync into `access.user_roles`, folded
/// into the same coarse buckets the website's NextAuth session used to
/// compute directly (`auth/vatsimProvider.ts::getRolesAndStaffPositions`).
pub const VATUSA_SYNCED_USER_ROLES: &[(&str, &str)] = &[
    ("ATM", "STAFF"),
    ("DATM", "STAFF"),
    ("TA", "STAFF"),
    ("EC", "STAFF"),
    ("FE", "STAFF"),
    ("WM", "STAFF"),
    ("INS", "INS"),
    ("MTR", "MTR"),
];

/// Sets a role from roster sync. Silently declines to overwrite a row a
/// human has manually touched (`source = 'manual'`) — mirrors
/// `crate::repos::users::set_staff_position_auto` exactly.
pub async fn set_user_role_auto(
    pool: &PgPool,
    user_id: &str,
    role_name: &str,
    held: bool,
) -> Result<(), ApiError> {
    if held {
        sqlx::query(
            r#"
            insert into access.user_roles (user_id, role_name, source, updated_at)
            values ($1, $2, 'auto', now())
            on conflict (user_id, role_name) do update
            set source = 'auto', updated_at = now()
            where access.user_roles.source = 'auto'
            "#,
        )
        .bind(user_id)
        .bind(role_name)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    } else {
        sqlx::query(
            r#"
            delete from access.user_roles
            where user_id = $1 and role_name = $2 and source = 'auto'
            "#,
        )
        .bind(user_id)
        .bind(role_name)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    }
    Ok(())
}

/// Reconciles the VATUSA-synced subset of `access.user_roles`
/// (`VATUSA_SYNCED_USER_ROLES`) against the facility roles observed for
/// this user during roster sync. `EVENT_STAFF` has no VATUSA facility-role
/// equivalent and is never touched here — manual-grant only.
pub async fn sync_user_roles_from_vatusa_roles(
    pool: &PgPool,
    user_id: &str,
    facility_roles: &[String],
) -> Result<(), ApiError> {
    for target_role in ["STAFF", "INS", "MTR"] {
        let held = VATUSA_SYNCED_USER_ROLES
            .iter()
            .any(|(vatusa, osmium)| *osmium == target_role && facility_roles.iter().any(|r| r == vatusa));
        set_user_role_auto(pool, user_id, target_role, held).await?;
    }
    Ok(())
}

/// The only coarse authorization roles assignable through the manual-grant
/// endpoint (`POST /api/v1/admin/users/{cid}/access`). `SERVER_ADMIN` is
/// exclusively claimed via the `OSMIUM_SERVER_ADMIN_CID` login path.
pub const ASSIGNABLE_USER_ROLES: &[&str] = &["STAFF", "INS", "MTR", "EVENT_STAFF"];

/// Sets a role from a manual admin action — always wins, marks the row
/// `source = 'manual'` so roster sync leaves it alone afterward. Mirrors
/// `crate::repos::users::set_staff_position_manual`. Runs in the caller's
/// transaction (unlike `set_user_role_auto`, which is pool-based/best-effort
/// for the background roster-sync job) so it commits atomically alongside
/// the permission changes on the same admin request.
pub async fn set_user_role_manual(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    role_name: &str,
    held: bool,
    updated_by: &str,
) -> Result<(), ApiError> {
    if held {
        sqlx::query(
            r#"
            insert into access.user_roles (user_id, role_name, source, updated_by, updated_at)
            values ($1, $2, 'manual', $3, now())
            on conflict (user_id, role_name) do update
            set source = 'manual', updated_by = excluded.updated_by, updated_at = now()
            "#,
        )
        .bind(user_id)
        .bind(role_name)
        .bind(updated_by)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    } else {
        sqlx::query("delete from access.user_roles where user_id = $1 and role_name = $2")
            .bind(user_id)
            .bind(role_name)
            .execute(&mut **tx)
            .await
            .map_err(|_| ApiError::Internal)?;
    }
    Ok(())
}

pub fn permission_names_to_permissions(
    permission_names: Vec<String>,
) -> Result<Vec<PermissionPath>, ApiError> {
    let mut permissions = Vec::with_capacity(permission_names.len());

    for name in permission_names {
        let Some(permission) = PermissionPath::from_db_value(&name) else {
            tracing::error!(permission_name = %name, "invalid permission value loaded from database");
            return Err(ApiError::Internal);
        };
        permissions.push(permission);
    }

    Ok(permissions)
}

/// Atomically flips `session_token`'s row into an impersonation of
/// `target_user_id` by `admin_user_id`, with a shortened TTL. The `where` clause
/// enforces the guards in one statement (no TOCTOU): the session must currently
/// belong to the admin and must not already be impersonating (refuses nesting).
/// Returns `true` when the row was updated, `false` when a guard rejected it.
pub async fn start_impersonation(
    pool: &PgPool,
    session_token: &str,
    target_user_id: &str,
    admin_user_id: &str,
    reason: Option<&str>,
    ttl_secs: i64,
) -> Result<bool, ApiError> {
    let result = sqlx::query(
        r#"
        update identity.sessions
        set user_id = $2,
            impersonator_user_id = $3,
            impersonation_started_at = now(),
            impersonation_reason = $4,
            expires_at = now() + make_interval(secs => $5)
        where session_token = $1
          and user_id = $3
          and impersonator_user_id is null
          and revoked_at is null
          and expires_at > now()
        "#,
    )
    .bind(session_token)
    .bind(target_user_id)
    .bind(admin_user_id)
    .bind(reason)
    .bind(ttl_secs as f64)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected() == 1)
}

/// Atomically restores an impersonation session back to the admin, clearing the
/// impersonation columns and restoring a normal TTL. Returns the restored admin
/// user id, or `None` if the session wasn't impersonating.
pub async fn stop_impersonation(
    pool: &PgPool,
    session_token: &str,
    restore_ttl_secs: i64,
) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        r#"
        update identity.sessions
        set user_id = impersonator_user_id,
            impersonator_user_id = null,
            impersonation_started_at = null,
            impersonation_reason = null,
            expires_at = now() + make_interval(secs => $2)
        where session_token = $1
          and impersonator_user_id is not null
        returning user_id
        "#,
    )
    .bind(session_token)
    .bind(restore_ttl_secs as f64)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Lists a user's active auth sessions (not revoked, not expired), newest first,
/// for the admin session manager. Never selects the raw `session_token`.
pub async fn list_user_sessions(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<crate::models::UserSessionItem>, ApiError> {
    sqlx::query_as::<_, crate::models::UserSessionItem>(
        r#"
        select
            s.id,
            host(s.ip_address) as ip_address,
            s.user_agent,
            s.created_at,
            s.expires_at,
            (s.impersonator_user_id is not null) as impersonated,
            imp.cid as impersonator_cid
        from identity.sessions s
        left join identity.users imp on imp.id = s.impersonator_user_id
        where s.user_id = $1
          and s.revoked_at is null
          and s.expires_at > now()
        order by s.created_at desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Revokes a single session, scoped to `user_id` so an admin can't revoke a session
/// that doesn't belong to the target user by guessing an id. Returns `true` if a
/// still-active session was revoked.
pub async fn revoke_user_session(
    pool: &PgPool,
    user_id: &str,
    session_id: &str,
) -> Result<bool, ApiError> {
    let result = sqlx::query(
        r#"
        update identity.sessions
        set revoked_at = now()
        where id = $1 and user_id = $2 and revoked_at is null
        "#,
    )
    .bind(session_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected() == 1)
}

/// Revokes all of a user's active sessions; returns how many were revoked.
pub async fn revoke_all_user_sessions(pool: &PgPool, user_id: &str) -> Result<u64, ApiError> {
    let result = sqlx::query(
        r#"
        update identity.sessions
        set revoked_at = now()
        where user_id = $1 and revoked_at is null and expires_at > now()
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(result.rows_affected())
}

/// Idempotently ensures a logging-in user has an audit actor row (`access.actors`,
/// `actor_type = 'user'`). Without this, `resolve_audit_actor` resolves null for every
/// human, so user-attributed audit entries and per-user IP history lose attribution.
/// Backed by the partial unique index from migration 0058.
pub async fn ensure_user_actor(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    display_name: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into access.actors (actor_type, user_id, display_name)
        values ('user', $1, $2)
        on conflict (user_id) where actor_type = 'user' do nothing
        "#,
    )
    .bind(user_id)
    .bind(display_name)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

/// Records a dossier entry on a user's log as a side effect of a permission
/// change. Moved verbatim out of `handlers/admin.rs::record_access_dossier_entry`
/// per spec 009 (kept in the access repo since it's an access-change side effect,
/// not a training action). Generates the id/timestamp internally, matching the
/// prior handler-level behavior.
pub async fn insert_access_dossier_entry(
    tx: &mut Transaction<'_, Postgres>,
    target_user_id: &str,
    writer_user_id: &str,
    message: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into feedback.dossier_entries (id, user_id, writer_id, message, timestamp, created_at)
        values ($1, $2, $3, $4, $5, $5)
        "#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(target_user_id)
    .bind(writer_user_id)
    .bind(message)
    .bind(chrono::Utc::now())
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();

    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use crate::errors::ApiError;

    use super::permission_names_to_permissions;

    #[test]
    fn rejects_invalid_database_permission_values() {
        let result = permission_names_to_permissions(vec!["not_a_permission".to_string()]);

        assert!(matches!(result, Err(ApiError::Internal)));
    }
}

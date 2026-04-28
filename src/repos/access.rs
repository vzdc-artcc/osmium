use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    auth::{
        acl::{PermissionKey, normalize_legacy_permission_name},
        context::{CurrentServiceAccount, CurrentUser},
    },
    errors::ApiError,
};

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
            pr.primary_role
        from identity.sessions s
        join identity.users u on u.id = s.user_id
        left join access.v_user_primary_role pr on pr.user_id = u.id
        where s.session_token = $1
          and s.revoked_at is null
          and s.expires_at > now()
        "#,
    )
    .bind(session_token)
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
            pr.primary_role
        from identity.users u
        left join access.v_user_primary_role pr on pr.user_id = u.id
        where u.cid = $1
        "#,
    )
    .bind(cid)
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

pub async fn replace_user_access(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    roles: &[String],
    permissions: &[(String, bool)],
) -> Result<(), ApiError> {
    if !roles.is_empty() {
        sqlx::query("delete from access.user_roles where user_id = $1")
            .bind(user_id)
            .execute(&mut **tx)
            .await
            .map_err(|_| ApiError::Internal)?;

        for role in roles {
            sqlx::query(
                r#"
                insert into access.user_roles (user_id, role_name)
                values ($1, $2)
                on conflict (user_id, role_name) do nothing
                "#,
            )
            .bind(user_id)
            .bind(role)
            .execute(&mut **tx)
            .await
            .map_err(|_| ApiError::Internal)?;
        }
    }

    if !permissions.is_empty() {
        sqlx::query("delete from access.user_permissions where user_id = $1")
            .bind(user_id)
            .execute(&mut **tx)
            .await
            .map_err(|_| ApiError::Internal)?;

        for (permission_name, granted) in permissions {
            sqlx::query(
                r#"
                insert into access.user_permissions (user_id, permission_name, granted)
                values ($1, $2, $3)
                on conflict (user_id, permission_name) do update
                set granted = excluded.granted
                "#,
            )
            .bind(user_id)
            .bind(permission_name)
            .bind(*granted)
            .execute(&mut **tx)
            .await
            .map_err(|_| ApiError::Internal)?;
        }
    }

    Ok(())
}

pub fn permission_names_to_permissions(
    permission_names: Vec<String>,
) -> Result<Vec<PermissionKey>, ApiError> {
    let mut permissions = Vec::with_capacity(permission_names.len());

    for name in permission_names {
        let canonical_name = normalize_legacy_permission_name(&name).unwrap_or(name);
        let Some(permission) = PermissionKey::from_db_value(&canonical_name) else {
            tracing::error!(permission_name = %canonical_name, "invalid permission value loaded from database");
            return Err(ApiError::Internal);
        };
        permissions.push(permission);
    }

    Ok(permissions)
}

fn sha256_hex(input: &str) -> String {
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

use sqlx::PgPool;

use crate::{errors::ApiError, models::CertificationItem};

pub async fn fetch_user_certifications(
    pool: &PgPool,
    cid: i64,
) -> Result<Vec<CertificationItem>, ApiError> {
    sqlx::query_as::<_, CertificationItem>(
        r#"
        select
            ct.id as certification_type_id,
            ct.name as certification_type_name,
            ct.sort_order,
            coalesce(uc.certification_option, 'NONE') as certification_option
        from org.certification_types ct
        join identity.users u on u.cid = $1
        left join org.user_certifications uc
            on uc.certification_type_id = ct.id and uc.user_id = u.id
        order by ct.sort_order asc, ct.name asc
        "#,
    )
    .bind(cid)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

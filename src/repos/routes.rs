use chrono::NaiveDate;
use sqlx::PgPool;

use crate::{errors::ApiError, models::routes::PreferredRouteItem};

/// A parsed preferred-route row ready to be written by the ingest job.
#[derive(Debug, Clone)]
pub struct PreferredRouteUpsert {
    pub origin: String,
    pub destination: String,
    pub route_type: String,
    pub sequence: i32,
    pub route_string: String,
    pub hours: String,
    pub area: String,
    pub altitude: String,
    pub aircraft: String,
    pub direction: String,
    pub departure_artcc: String,
    pub arrival_artcc: String,
}

/// Search locally-owned preferred routes by origin and/or destination. Identifiers
/// are stored upper-cased, so callers must upper-case their inputs before binding.
/// Returns routes ordered by origin, destination, then FAA route sequence.
pub async fn search_preferred_routes(
    pool: &PgPool,
    origin: Option<&str>,
    destination: Option<&str>,
) -> Result<Vec<PreferredRouteItem>, ApiError> {
    sqlx::query_as::<_, PreferredRouteItem>(
        r#"
        select
            origin,
            destination,
            route_type,
            sequence,
            route_string,
            hours,
            area,
            altitude,
            aircraft,
            direction,
            departure_artcc,
            arrival_artcc,
            effective_date
        from routes.preferred_routes
        where ($1::text is null or origin = $1)
          and ($2::text is null or destination = $2)
        order by origin asc, destination asc, sequence asc
        limit 1000
        "#,
    )
    .bind(origin)
    .bind(destination)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// The newest cycle effective date currently loaded, used by the ingest job to
/// skip re-downloading a cycle that is already present.
pub async fn latest_effective_date(pool: &PgPool) -> Result<Option<NaiveDate>, ApiError> {
    sqlx::query_scalar::<_, Option<NaiveDate>>(
        "select max(effective_date) from routes.preferred_routes",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Wholesale-replace the preferred-routes table with a freshly-ingested NASR cycle.
/// Runs in a single transaction so the search endpoint never observes a partial
/// (half-deleted / half-inserted) dataset. Returns the number of rows written.
///
/// Wholesale replacement (rather than per-row upsert) is deliberate: it naturally
/// drops routes the FAA removed in the new cycle, which an upsert-only path would
/// leave behind as stale.
pub async fn replace_all_preferred_routes(
    pool: &PgPool,
    routes: &[PreferredRouteUpsert],
    effective_date: Option<NaiveDate>,
) -> Result<usize, ApiError> {
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    sqlx::query("delete from routes.preferred_routes")
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;

    let mut written = 0usize;
    for route in routes {
        sqlx::query(
            r#"
            insert into routes.preferred_routes (
                origin, destination, route_type, sequence, route_string,
                hours, area, altitude, aircraft, direction,
                departure_artcc, arrival_artcc, effective_date
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            on conflict (origin, destination, route_type, sequence) do update
            set route_string = excluded.route_string,
                hours = excluded.hours,
                area = excluded.area,
                altitude = excluded.altitude,
                aircraft = excluded.aircraft,
                direction = excluded.direction,
                departure_artcc = excluded.departure_artcc,
                arrival_artcc = excluded.arrival_artcc,
                effective_date = excluded.effective_date,
                updated_at = now()
            "#,
        )
        .bind(&route.origin)
        .bind(&route.destination)
        .bind(&route.route_type)
        .bind(route.sequence)
        .bind(&route.route_string)
        .bind(&route.hours)
        .bind(&route.area)
        .bind(&route.altitude)
        .bind(&route.aircraft)
        .bind(&route.direction)
        .bind(&route.departure_artcc)
        .bind(&route.arrival_artcc)
        .bind(effective_date)
        .execute(&mut *tx)
        .await
        .map_err(crate::repos::map_constraint_error)?;
        written += 1;
    }

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(written)
}

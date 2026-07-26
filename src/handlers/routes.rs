use axum::extract::{Extension, Query, State};

use crate::{
    auth::context::CurrentUser,
    errors::ApiError,
    models::routes::{PreferredRouteItem, PreferredRoutesQuery, PreferredRoutesResponse},
    repos::routes as routes_repo,
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

/// Search locally-owned FAA preferred IFR routes.
///
/// Authenticated controllers only (not a specific permission — this is public FAA
/// reference data any logged-in member can use for flight planning, but it is
/// deliberately not exposed unauthenticated). At least one of `origin`/`destination`
/// must be supplied. The data is served from osmium's own copy (refreshed by the
/// `faa_preferred_routes` ingest job), replacing the removed request-time proxy of a
/// third-party aviation API.
#[utoipa::path(
    get,
    path = "/api/v1/routes/preferred",
    tag = "routes",
    params(PreferredRoutesQuery),
    responses(
        (status = 200, description = "Matching preferred routes", body = PreferredRoutesResponse),
        (status = 400, description = "Neither origin nor destination supplied"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn search_preferred_routes(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
    Query(params): Query<PreferredRoutesQuery>,
) -> Result<ApiJson<PreferredRoutesResponse>, ApiError> {
    // Authenticated members only — reference data, but not unauthenticated.
    current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let origin = normalize_identifier(params.origin.as_deref());
    let destination = normalize_identifier(params.destination.as_deref());

    // Require at least one search term — an unfiltered dump of every route is
    // both meaningless to the caller and a needless load.
    if origin.is_none() && destination.is_none() {
        return Err(ApiError::BadRequest);
    }

    let routes: Vec<PreferredRouteItem> =
        routes_repo::search_preferred_routes(pool, origin.as_deref(), destination.as_deref())
            .await?;

    let count = routes.len();
    Ok(ApiJson::new(PreferredRoutesResponse { routes, count }, time))
}

/// Trim, upper-case, and drop empty airport identifiers so comparison against the
/// upper-cased stored values is case-insensitive.
fn normalize_identifier(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
}

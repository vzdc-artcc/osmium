use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// One locally-owned FAA preferred IFR route, mirroring the columns the old PRD
/// page displayed. Sourced from the NFDC NASR `PFR_RMT_FMT.csv` on a 28-day cadence.
#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct PreferredRouteItem {
    pub origin: String,
    pub destination: String,
    /// FAA preferred-route type code (e.g. `TEC`, `L`, `H`, `LDR`, `HDR`, `SEA`).
    pub route_type: String,
    pub sequence: i32,
    /// The assembled route string (fixes/airways/navaids), origin→destination.
    pub route_string: String,
    pub hours: String,
    pub area: String,
    pub altitude: String,
    pub aircraft: String,
    /// Direction/flow limitation, if any.
    pub direction: String,
    /// Departing ARTCC boundary (e.g. `ZDC`).
    pub departure_artcc: String,
    /// Arriving ARTCC boundary.
    pub arrival_artcc: String,
    /// The 28-day NASR cycle effective date this route was ingested from.
    pub effective_date: Option<chrono::NaiveDate>,
}

/// Query parameters for `GET /api/v1/routes/preferred`. At least one of `origin`
/// or `destination` must be supplied; both are matched case-insensitively.
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct PreferredRoutesQuery {
    /// Origin airport identifier (e.g. `KJFK` or `JFK`).
    pub origin: Option<String>,
    /// Destination airport identifier.
    pub destination: Option<String>,
}

/// Response envelope for a preferred-routes search.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PreferredRoutesResponse {
    pub routes: Vec<PreferredRouteItem>,
    pub count: usize,
}

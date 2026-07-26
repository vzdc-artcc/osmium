use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// A single ATC booking as returned by the upstream VATSIM ATC-booking service
/// (`atc-bookings.vatsim.net`). Times are the upstream `"YYYY-MM-DD HH:mm:ss"`
/// UTC strings, passed through untouched. Mirrors `website/lib/atcBooking.ts`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AtcBookingItem {
    pub id: i64,
    pub callsign: String,
    pub cid: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub division: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdivision: Option<String>,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AtcBookingListResponse {
    pub items: Vec<AtcBookingItem>,
}

/// Create/update body. `id` is taken from the path on update; the upstream body
/// never carries it. `type` defaults to a normal booking; `"training"` bypasses
/// the self-booking limits (used by the training-appointment scheduler).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateOrUpdateAtcBookingRequest {
    pub callsign: String,
    pub cid: i64,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub division: Option<String>,
    #[serde(default)]
    pub subdivision: Option<String>,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListAtcBookingsQuery {
    /// Restrict to one controller's bookings; omit for the ARTCC-wide calendar.
    pub cid: Option<i64>,
}

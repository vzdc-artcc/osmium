//! Scheduled ingest of the FAA's own preferred IFR routes (spec 012, Worker B).
//!
//! The old site proxied a third-party aviation API at request time (removed in
//! `0855eb0`). This instead downloads the FAA National Flight Data Center's own
//! copy — published on a 28-day AIRAC cadence as part of the NASR subscription —
//! and serves search over a locally-owned table (`routes.preferred_routes`), so
//! the API stops taking a runtime dependency on a third party.
//!
//! Source (confirmed against nfdc.faa.gov, 2026-07 cycle):
//!   `https://nfdc.faa.gov/webContent/28DaySub/extra/{DD}_{Mon}_{YYYY}_PFR_CSV.zip`
//! The zip contains `PFR_RMT_FMT.csv` (the "RMT format"), whose columns map almost
//! 1:1 onto the fields the old PRD UI displayed:
//!   Orig, Route String, Dest, Hours1, Type, Area, Altitude, Aircraft,
//!   Direction, Seq, DCNTR (departure ARTCC), ACNTR (arrival ARTCC).
//!
//! NOTE: the FAA has announced a NASR file-format change effective the 2026-09-03
//! AIRAC cycle. The CSV parser here is therefore **header-driven** (it maps by
//! column *name*, not position) and fails loudly if a required column disappears,
//! rather than silently mis-parsing. The download URL is env-overridable so a
//! moved source can be repointed without a code change.

use std::io::Read;
use std::time::Duration;

use chrono::{Datelike, NaiveDate, Utc};
use serde_json::json;

use crate::{
    errors::ApiError,
    handlers::org::JobExecutionSummary,
    jobs::{Job, TickOutcome},
    repos::routes::{self as routes_repo, PreferredRouteUpsert},
    state::AppState,
};

/// Default cadence: check daily, but only actually download when the current
/// 28-day cycle's data isn't already loaded (see [`run_sync`]).
const DEFAULT_INTERVAL_SECS: u64 = 86_400;
const DEFAULT_BASE_URL: &str = "https://nfdc.faa.gov/webContent/28DaySub/extra";
const DEFAULT_CSV_NAME: &str = "PFR_RMT_FMT.csv";

/// A known NASR 28-day cycle effective date, used as the anchor for computing the
/// current cycle. Cycles recur exactly every 28 days (AIRAC). Verified against the
/// FAA subscription index: 2026-05-14, 2026-06-11, 2026-07-09, 2026-08-06, …
const CYCLE_ANCHOR: (i32, u32, u32) = (2026, 5, 14);
const CYCLE_DAYS: i64 = 28;

#[derive(Debug, Clone, Default)]
pub struct FaaPreferredRoutesMetrics {
    pub routes_written: Option<i64>,
    pub effective_date: Option<String>,
    pub skipped_already_current: Option<bool>,
}

#[derive(Clone)]
struct FaaPreferredRoutesConfig {
    interval_secs: u64,
    /// Full URL override (skips cycle-date computation) — handy for a mirror or test.
    url_override: Option<String>,
    base_url: String,
    csv_name: String,
}

struct FaaPreferredRoutesJob {
    config: FaaPreferredRoutesConfig,
}

impl Job for FaaPreferredRoutesJob {
    type Metrics = FaaPreferredRoutesMetrics;

    fn name(&self) -> &'static str {
        "faa_preferred_routes"
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(self.config.interval_secs)
    }

    async fn tick(&self, state: &AppState) -> Result<TickOutcome<Self::Metrics>, String> {
        match run_sync(state, &self.config).await {
            Ok(metrics) => Ok(TickOutcome::success(metrics)),
            Err(err) => Err(err.to_string()),
        }
    }
}

pub fn start_faa_preferred_routes_worker(state: AppState) {
    if !sync_enabled() {
        if let Ok(mut health) = state.job_health.write() {
            health.faa_preferred_routes.enabled = false;
            health.faa_preferred_routes.last_error = None;
        }
        tracing::info!("faa preferred routes worker disabled");
        return;
    }

    let config = config_from_env();
    if let Ok(mut health) = state.job_health.write() {
        health.faa_preferred_routes.enabled = true;
        health.faa_preferred_routes.last_error = None;
    }
    tracing::info!(
        interval_secs = config.interval_secs,
        "starting faa preferred routes worker"
    );

    let job_health = state.job_health.clone();
    crate::jobs::spawn(FaaPreferredRoutesJob { config }, state, job_health, |health| {
        &mut health.faa_preferred_routes
    });
}

/// Manual-trigger entry point for `POST /admin/jobs/faa_preferred_routes/run`.
pub(crate) async fn execute_faa_preferred_routes_sync(
    state: &AppState,
    _pool: &sqlx::PgPool,
) -> Result<JobExecutionSummary, ApiError> {
    let config = config_from_env();
    let metrics = run_sync(state, &config).await.map_err(|err| {
        tracing::warn!(error = %err, "manual faa preferred routes sync failed");
        err.into_api_error()
    })?;

    Ok(JobExecutionSummary {
        processed: metrics.routes_written.unwrap_or(0),
        details: json!({
            "routes_written": metrics.routes_written,
            "effective_date": metrics.effective_date,
            "skipped_already_current": metrics.skipped_already_current,
        }),
    })
}

/// Sync error with a human string (for job health) and a mapping back to `ApiError`
/// for the manual-run path.
#[derive(Debug)]
enum SyncError {
    DatabaseUnavailable,
    Download(String),
    Parse(String),
    Db,
}

impl SyncError {
    fn into_api_error(self) -> ApiError {
        match self {
            SyncError::DatabaseUnavailable => ApiError::ServiceUnavailable,
            SyncError::Download(_) | SyncError::Parse(_) => ApiError::ServiceUnavailable,
            SyncError::Db => ApiError::Internal,
        }
    }
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::DatabaseUnavailable => write!(f, "database unavailable"),
            SyncError::Download(msg) => write!(f, "download failed: {msg}"),
            SyncError::Parse(msg) => write!(f, "parse failed: {msg}"),
            SyncError::Db => write!(f, "database write failed"),
        }
    }
}

async fn run_sync(
    state: &AppState,
    config: &FaaPreferredRoutesConfig,
) -> Result<FaaPreferredRoutesMetrics, SyncError> {
    let pool = state.db.as_ref().ok_or(SyncError::DatabaseUnavailable)?;

    let today = Utc::now().date_naive();
    let cycle = current_cycle_date(today);
    let url = config
        .url_override
        .clone()
        .unwrap_or_else(|| build_cycle_url(&config.base_url, cycle));

    // Skip the (large) download when this cycle's data is already loaded — makes a
    // daily tick cheap and restart-safe.
    let existing = routes_repo::latest_effective_date(pool)
        .await
        .map_err(|_| SyncError::Db)?;
    if existing == Some(cycle) {
        tracing::info!(effective_date = %cycle, "faa preferred routes already current; skipping download");
        return Ok(FaaPreferredRoutesMetrics {
            routes_written: Some(0),
            effective_date: Some(cycle.to_string()),
            skipped_already_current: Some(true),
        });
    }

    tracing::info!(%url, effective_date = %cycle, "downloading faa preferred routes");
    let zip_bytes = download(&url).await.map_err(SyncError::Download)?;
    let csv_text = extract_csv_from_zip(&zip_bytes, &config.csv_name).map_err(SyncError::Parse)?;
    let routes = parse_rmt_csv(&csv_text).map_err(SyncError::Parse)?;

    if routes.is_empty() {
        return Err(SyncError::Parse(
            "parsed zero routes from PFR CSV (unexpected)".to_string(),
        ));
    }

    let written = routes_repo::replace_all_preferred_routes(pool, &routes, Some(cycle))
        .await
        .map_err(|_| SyncError::Db)?;

    tracing::info!(
        routes_written = written,
        effective_date = %cycle,
        "faa preferred routes sync completed"
    );

    Ok(FaaPreferredRoutesMetrics {
        routes_written: Some(written as i64),
        effective_date: Some(cycle.to_string()),
        skipped_already_current: Some(false),
    })
}

async fn download(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("osmium-faa-preferred-routes-sync")
        .build()
        .map_err(|err| err.to_string())?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {status} from {url}"));
    }

    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|err| err.to_string())
}

/// Extract a single named CSV entry from the downloaded NASR zip. Uses the `zip`
/// crate (deflate) — the NASR PFR entries are deflate-compressed.
fn extract_csv_from_zip(zip_bytes: &[u8], csv_name: &str) -> Result<String, String> {
    let reader = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|err| format!("open zip: {err}"))?;

    let mut file = archive
        .by_name(csv_name)
        .map_err(|err| format!("entry {csv_name} not found in zip: {err}"))?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|err| format!("read {csv_name}: {err}"))?;
    Ok(contents)
}

/// Parse the FAA `PFR_RMT_FMT.csv` into upsert rows. Header-driven: the first
/// record names the columns, and every subsequent record is indexed by column
/// name so a future column reorder (the 2026-09-03 NASR format change) does not
/// silently corrupt the data. Fails if any required column is missing.
fn parse_rmt_csv(text: &str) -> Result<Vec<PreferredRouteUpsert>, String> {
    let mut records = parse_csv_records(text).into_iter();

    let header = records.next().ok_or_else(|| "empty CSV".to_string())?;
    let index = HeaderIndex::from_header(&header)?;

    let mut routes = Vec::new();
    for record in records {
        // Skip trailing blank lines.
        if record.iter().all(|field| field.trim().is_empty()) {
            continue;
        }

        let origin = index.get(&record, Column::Origin).trim().to_ascii_uppercase();
        let destination = index
            .get(&record, Column::Destination)
            .trim()
            .to_ascii_uppercase();
        let route_type = index.get(&record, Column::Type).trim().to_ascii_uppercase();
        let sequence = index
            .get(&record, Column::Sequence)
            .trim()
            .parse::<i32>()
            .unwrap_or(0);

        // A route with neither endpoint is unusable as a search key — skip it.
        if origin.is_empty() && destination.is_empty() {
            continue;
        }

        routes.push(PreferredRouteUpsert {
            origin,
            destination,
            route_type,
            sequence,
            route_string: index.get(&record, Column::RouteString).trim().to_string(),
            hours: index.get(&record, Column::Hours).trim().to_string(),
            area: index.get(&record, Column::Area).trim().to_string(),
            altitude: index.get(&record, Column::Altitude).trim().to_string(),
            aircraft: index.get(&record, Column::Aircraft).trim().to_string(),
            direction: index.get(&record, Column::Direction).trim().to_string(),
            departure_artcc: index
                .get(&record, Column::DepartureArtcc)
                .trim()
                .to_ascii_uppercase(),
            arrival_artcc: index
                .get(&record, Column::ArrivalArtcc)
                .trim()
                .to_ascii_uppercase(),
        });
    }

    // De-duplicate on the natural key (the FAA data can contain the same route
    // key twice across formats); last write wins, matching the DB upsert. A stable
    // sort keeps equal-key rows in input order; reversing before/after the dedup
    // (which keeps the *first* of each run) makes it keep the *last* input row.
    routes.sort_by(|a, b| {
        (&a.origin, &a.destination, &a.route_type, a.sequence).cmp(&(
            &b.origin,
            &b.destination,
            &b.route_type,
            b.sequence,
        ))
    });
    routes.reverse();
    routes.dedup_by(|a, b| {
        a.origin == b.origin
            && a.destination == b.destination
            && a.route_type == b.route_type
            && a.sequence == b.sequence
    });
    routes.reverse();

    Ok(routes)
}

#[derive(Clone, Copy)]
enum Column {
    Origin,
    Destination,
    Type,
    Sequence,
    RouteString,
    Hours,
    Area,
    Altitude,
    Aircraft,
    Direction,
    DepartureArtcc,
    ArrivalArtcc,
}

impl Column {
    /// Accepted header names for this column, lower-cased. First is the current
    /// (2026) name; extras give the parser a little forward/backward tolerance.
    fn aliases(self) -> &'static [&'static str] {
        match self {
            Column::Origin => &["orig", "origin", "origin_id"],
            Column::Destination => &["dest", "destination", "dstn_id"],
            Column::Type => &["type", "pfr_type_code", "route_type"],
            Column::Sequence => &["seq", "route_no", "sequence"],
            Column::RouteString => &["route string", "route_string", "route"],
            Column::Hours => &["hours1", "hours", "hours_1"],
            Column::Area => &["area", "special_area_descrip"],
            Column::Altitude => &["altitude", "alt_descrip"],
            Column::Aircraft => &["aircraft"],
            Column::Direction => &["direction", "route_dir_descrip", "flow"],
            Column::DepartureArtcc => &["dcntr", "departure_artcc", "dep_artcc"],
            Column::ArrivalArtcc => &["acntr", "arrival_artcc", "arr_artcc"],
        }
    }

    fn label(self) -> &'static str {
        self.aliases()[0]
    }
}

struct HeaderIndex {
    origin: usize,
    destination: usize,
    ty: usize,
    sequence: usize,
    route_string: Option<usize>,
    hours: Option<usize>,
    area: Option<usize>,
    altitude: Option<usize>,
    aircraft: Option<usize>,
    direction: Option<usize>,
    departure_artcc: Option<usize>,
    arrival_artcc: Option<usize>,
}

impl HeaderIndex {
    fn from_header(header: &[String]) -> Result<Self, String> {
        let normalized: Vec<String> = header
            .iter()
            .map(|h| h.trim().to_ascii_lowercase())
            .collect();

        let find = |column: Column| -> Option<usize> {
            column
                .aliases()
                .iter()
                .find_map(|alias| normalized.iter().position(|h| h == alias))
        };

        let require = |column: Column| -> Result<usize, String> {
            find(column).ok_or_else(|| {
                format!(
                    "required column '{}' not found in PFR CSV header (found: {:?})",
                    column.label(),
                    normalized
                )
            })
        };

        Ok(HeaderIndex {
            origin: require(Column::Origin)?,
            destination: require(Column::Destination)?,
            ty: require(Column::Type)?,
            sequence: require(Column::Sequence)?,
            route_string: find(Column::RouteString),
            hours: find(Column::Hours),
            area: find(Column::Area),
            altitude: find(Column::Altitude),
            aircraft: find(Column::Aircraft),
            direction: find(Column::Direction),
            departure_artcc: find(Column::DepartureArtcc),
            arrival_artcc: find(Column::ArrivalArtcc),
        })
    }

    fn get<'a>(&self, record: &'a [String], column: Column) -> &'a str {
        let idx = match column {
            Column::Origin => Some(self.origin),
            Column::Destination => Some(self.destination),
            Column::Type => Some(self.ty),
            Column::Sequence => Some(self.sequence),
            Column::RouteString => self.route_string,
            Column::Hours => self.hours,
            Column::Area => self.area,
            Column::Altitude => self.altitude,
            Column::Aircraft => self.aircraft,
            Column::Direction => self.direction,
            Column::DepartureArtcc => self.departure_artcc,
            Column::ArrivalArtcc => self.arrival_artcc,
        };
        idx.and_then(|i| record.get(i))
            .map(String::as_str)
            .unwrap_or("")
    }
}

/// Minimal RFC-4180 CSV reader (quoted fields, embedded commas/newlines, `""`
/// escapes). Hand-rolled to avoid pulling in a `csv` crate dependency for one
/// well-behaved FAA file.
fn parse_csv_records(text: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut field = String::new();
    let mut record: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        field.push('"');
                        chars.next();
                    } else {
                        in_quotes = false;
                    }
                }
                _ => field.push(c),
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    record.push(std::mem::take(&mut field));
                }
                '\r' => {}
                '\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                }
                _ => field.push(c),
            }
        }
    }

    // Flush a trailing record with no final newline.
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    records
}

fn current_cycle_date(today: NaiveDate) -> NaiveDate {
    let anchor = NaiveDate::from_ymd_opt(CYCLE_ANCHOR.0, CYCLE_ANCHOR.1, CYCLE_ANCHOR.2)
        .expect("valid cycle anchor");
    let days_since = (today - anchor).num_days();
    let cycles = days_since.div_euclid(CYCLE_DAYS);
    anchor + chrono::Duration::days(cycles * CYCLE_DAYS)
}

fn build_cycle_url(base_url: &str, cycle: NaiveDate) -> String {
    // FAA URL date format: zero-padded day, 3-letter English month, full year,
    // e.g. "09_Jul_2026" → .../extra/09_Jul_2026_PFR_CSV.zip
    let month = MONTH_ABBR[(cycle.month0()) as usize];
    format!(
        "{}/{:02}_{}_{}_PFR_CSV.zip",
        base_url.trim_end_matches('/'),
        cycle.day(),
        month,
        cycle.year()
    )
}

const MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn config_from_env() -> FaaPreferredRoutesConfig {
    FaaPreferredRoutesConfig {
        interval_secs: std::env::var("FAA_PREFERRED_ROUTES_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_INTERVAL_SECS)
            .max(60),
        url_override: std::env::var("FAA_PREFERRED_ROUTES_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        base_url: std::env::var("FAA_PREFERRED_ROUTES_BASE_URL")
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        csv_name: std::env::var("FAA_PREFERRED_ROUTES_CSV_NAME")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_CSV_NAME.to_string()),
    }
}

/// Opt-in: this is a new outbound integration, disabled unless explicitly enabled,
/// so no environment starts hitting the FAA by surprise.
fn sync_enabled() -> bool {
    std::env::var("FAA_PREFERRED_ROUTES_SYNC_ENABLED")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_date_is_anchor_on_anchor_day() {
        let anchor = NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        assert_eq!(current_cycle_date(anchor), anchor);
    }

    #[test]
    fn cycle_date_rounds_down_within_a_cycle() {
        // 2026-07-25 falls in the cycle starting 2026-07-09.
        let today = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
        assert_eq!(
            current_cycle_date(today),
            NaiveDate::from_ymd_opt(2026, 7, 9).unwrap()
        );
    }

    #[test]
    fn cycle_date_advances_every_28_days() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 6).unwrap();
        assert_eq!(
            current_cycle_date(today),
            NaiveDate::from_ymd_opt(2026, 8, 6).unwrap()
        );
    }

    #[test]
    fn cycle_date_handles_dates_before_anchor() {
        // 2026-04-16 is exactly one cycle before the anchor.
        let today = NaiveDate::from_ymd_opt(2026, 4, 20).unwrap();
        assert_eq!(
            current_cycle_date(today),
            NaiveDate::from_ymd_opt(2026, 4, 16).unwrap()
        );
    }

    #[test]
    fn url_uses_zero_padded_day_and_english_month() {
        let cycle = NaiveDate::from_ymd_opt(2026, 7, 9).unwrap();
        assert_eq!(
            build_cycle_url("https://nfdc.faa.gov/webContent/28DaySub/extra", cycle),
            "https://nfdc.faa.gov/webContent/28DaySub/extra/09_Jul_2026_PFR_CSV.zip"
        );
    }

    #[test]
    fn url_trims_trailing_slash_on_base() {
        let cycle = NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        assert_eq!(
            build_cycle_url("https://example.test/base/", cycle),
            "https://example.test/base/14_May_2026_PFR_CSV.zip"
        );
    }

    #[test]
    fn parses_rmt_csv_header_driven() {
        let csv = "\"Orig\",\"Route String\",\"Dest\",\"Hours1\",\"Type\",\"Area\",\"Altitude\",\"Aircraft\",\"Direction\",\"Seq\",\"DCNTR\",\"ACNTR\"\n\
\"ABE\",\"ABE FJC ARD CYN ACY\",\"ACY\",\"\",\"TEC\",\"\",\"5000\",\"\",\"\",1,\"ZNY\",\"ZDC\"\n\
\"ABE\",\"ABE FJC LAAYK ALB\",\"ALB\",\"\",\"TEC\",\"TO ALB,SCH\",\"7000\",\"\",\"\",1,\"ZNY\",\"ZBW\"\n";
        let routes = parse_rmt_csv(csv).expect("parse ok");
        assert_eq!(routes.len(), 2);

        let first = &routes[0];
        assert_eq!(first.origin, "ABE");
        assert_eq!(first.destination, "ACY");
        assert_eq!(first.route_type, "TEC");
        assert_eq!(first.sequence, 1);
        assert_eq!(first.route_string, "ABE FJC ARD CYN ACY");
        assert_eq!(first.altitude, "5000");
        assert_eq!(first.departure_artcc, "ZNY");
        assert_eq!(first.arrival_artcc, "ZDC");

        // Embedded comma inside a quoted field must be preserved.
        let second = &routes[1];
        assert_eq!(second.destination, "ALB");
        assert_eq!(second.area, "TO ALB,SCH");
    }

    #[test]
    fn parse_tolerates_column_reorder_by_name() {
        let csv = "\"Type\",\"Seq\",\"Dest\",\"Orig\",\"Route String\"\n\
\"L\",5,\"KACY\",\"kjfk\",\"J174 X\"\n";
        let routes = parse_rmt_csv(csv).expect("parse ok");
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].origin, "KJFK");
        assert_eq!(routes[0].destination, "KACY");
        assert_eq!(routes[0].route_type, "L");
        assert_eq!(routes[0].sequence, 5);
        assert_eq!(routes[0].route_string, "J174 X");
    }

    #[test]
    fn parse_fails_when_required_column_missing() {
        // No "Seq"/"Route_No" column.
        let csv = "\"Orig\",\"Dest\",\"Type\"\n\"KJFK\",\"KACY\",\"L\"\n";
        assert!(parse_rmt_csv(csv).is_err());
    }

    #[test]
    fn parse_deduplicates_on_natural_key() {
        let csv = "\"Orig\",\"Dest\",\"Type\",\"Seq\",\"Route String\"\n\
\"KJFK\",\"KACY\",\"L\",1,\"OLD\"\n\
\"KJFK\",\"KACY\",\"L\",1,\"NEW\"\n";
        let routes = parse_rmt_csv(csv).expect("parse ok");
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].route_string, "NEW");
    }
}

use chrono::{DateTime, TimeZone, Utc};
use sqlx::PgPool;

use crate::{
    errors::ApiError,
    models::{
        TrainingStatsBundle, TrainingStatsLessonDistribution, TrainingStatsMonthlyBucket,
        TrainingStatsMostRunLesson, TrainingStatsTopTrainer,
    },
};

const MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Half-open UTC window `[start, next)` for a 0-based month within `year`.
fn month_window(year: i32, month0: u32) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = Utc.with_ymd_and_hms(year, month0 + 1, 1, 0, 0, 0).unwrap();
    let (ny, nm) = if month0 == 11 {
        (year + 1, 1)
    } else {
        (year, month0 + 2)
    };
    let next = Utc.with_ymd_and_hms(ny, nm, 1, 0, 0, 0).unwrap();
    (start, next)
}

/// Half-open UTC window `[start, next)` for a full year.
fn year_window(year: i32) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0).unwrap();
    let next = Utc.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0).unwrap();
    (start, next)
}

fn empty_bundle(month: Option<i32>) -> TrainingStatsBundle {
    TrainingStatsBundle {
        sessions: 0,
        total_hours: 0.0,
        passed: 0,
        failed: 0,
        most_run_lesson: TrainingStatsMostRunLesson {
            identifier: None,
            count: 0,
        },
        top_trainers: Vec::new(),
        monthly_sessions: if month.is_none() {
            (0..12)
                .map(|i| TrainingStatsMonthlyBucket {
                    month: MONTH_ABBR[i].to_string(),
                    sessions: 0,
                })
                .collect()
        } else {
            Vec::new()
        },
        lesson_distribution: Vec::new(),
    }
}

/// All-time sum of training-session durations, in hours. Backs the statistics
/// layout's "All-Time Hours" card.
pub async fn all_time_training_hours(pool: &PgPool) -> Result<f64, ApiError> {
    sqlx::query_scalar::<_, f64>(
        r#"
        select coalesce(sum(extract(epoch from (s."end" - s.start)) / 3600.0), 0)::double precision
        from training.training_sessions s
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Aggregated training-session metrics for a scope: facility-wide or scoped to
/// one instructor (`cid`), over a full year or a single 0-based `month`.
pub async fn training_stats_bundle(
    pool: &PgPool,
    year: i32,
    month: Option<i32>,
    cid: Option<i64>,
) -> Result<TrainingStatsBundle, ApiError> {
    // Guard the window builders: `year` is an unvalidated request param, and an
    // out-of-range value would overflow `year + 1` or make chrono's date
    // construction return None and panic on `.unwrap()`.
    if !(1970..=9999).contains(&year) {
        return Err(ApiError::BadRequest);
    }

    let (start, next) = match month {
        Some(m) if (0..=11).contains(&m) => month_window(year, m as u32),
        Some(_) => return Err(ApiError::BadRequest),
        None => year_window(year),
    };

    // Resolve the instructor filter once; an unknown cid yields an empty bundle.
    let instructor_id: Option<String> = match cid {
        Some(c) => {
            let resolved =
                sqlx::query_scalar::<_, String>("select id from identity.users where cid = $1")
                    .bind(c)
                    .fetch_optional(pool)
                    .await
                    .map_err(|_| ApiError::Internal)?;
            match resolved {
                Some(id) => Some(id),
                None => return Ok(empty_bundle(month)),
            }
        }
        None => None,
    };

    let filter = instructor_id.as_deref();

    let (sessions, total_hours) = sqlx::query_as::<_, (i64, f64)>(
        r#"
        select
            count(*)::bigint as sessions,
            coalesce(sum(extract(epoch from (s."end" - s.start)) / 3600.0), 0)::double precision as total_hours
        from training.training_sessions s
        where s.start >= $1 and s.start < $2
          and ($3::text is null or s.instructor_id = $3)
        "#,
    )
    .bind(start)
    .bind(next)
    .bind(filter)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let (passed, failed) = sqlx::query_as::<_, (i64, i64)>(
        r#"
        select
            count(*) filter (where not has_fail)::bigint as passed,
            count(*) filter (where has_fail)::bigint as failed
        from (
            select
                s.id,
                exists(
                    select 1 from training.training_tickets t
                    where t.session_id = s.id and t.passed = false
                ) as has_fail
            from training.training_sessions s
            where s.start >= $1 and s.start < $2
              and ($3::text is null or s.instructor_id = $3)
        ) sub
        "#,
    )
    .bind(start)
    .bind(next)
    .bind(filter)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let most_run = sqlx::query_as::<_, (String, i64)>(
        r#"
        select l.identifier, count(*)::bigint as cnt
        from training.training_tickets t
        join training.training_sessions s on s.id = t.session_id
        join training.lessons l on l.id = t.lesson_id
        where s.start >= $1 and s.start < $2
          and ($3::text is null or s.instructor_id = $3)
        group by l.identifier
        order by cnt desc, l.identifier asc
        limit 1
        "#,
    )
    .bind(start)
    .bind(next)
    .bind(filter)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let most_run_lesson = match most_run {
        Some((identifier, count)) => TrainingStatsMostRunLesson {
            identifier: Some(identifier),
            count,
        },
        None => TrainingStatsMostRunLesson {
            identifier: None,
            count: 0,
        },
    };

    let lesson_distribution = sqlx::query_as::<_, TrainingStatsLessonDistribution>(
        r#"
        select
            coalesce(nullif(l.identifier, ''), l.name) as lesson,
            count(*) filter (where t.passed)::bigint as passed,
            count(*) filter (where not t.passed)::bigint as failed
        from training.training_tickets t
        join training.training_sessions s on s.id = t.session_id
        join training.lessons l on l.id = t.lesson_id
        where s.start >= $1 and s.start < $2
          and ($3::text is null or s.instructor_id = $3)
        group by coalesce(nullif(l.identifier, ''), l.name)
        order by lesson asc
        "#,
    )
    .bind(start)
    .bind(next)
    .bind(filter)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    // Top trainers only make sense facility-wide (no instructor filter).
    let top_trainers = if filter.is_none() {
        sqlx::query_as::<_, TrainingStatsTopTrainer>(
            r#"
            select
                s.instructor_id as id,
                u.cid,
                u.first_name,
                u.last_name,
                u.preferred_name,
                u.display_name,
                coalesce(sum(extract(epoch from (s."end" - s.start)) / 3600.0), 0)::double precision as hours
            from training.training_sessions s
            join identity.users u on u.id = s.instructor_id
            where s.start >= $1 and s.start < $2
            group by s.instructor_id, u.cid, u.first_name, u.last_name, u.preferred_name, u.display_name
            order by hours desc, u.cid asc
            limit 3
            "#,
        )
        .bind(start)
        .bind(next)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?
    } else {
        Vec::new()
    };

    // Monthly session counts only make sense for a full-year scope.
    let monthly_sessions = if month.is_none() {
        let rows = sqlx::query_as::<_, (i32, i64)>(
            r#"
            select extract(month from s.start)::int as m, count(*)::bigint as cnt
            from training.training_sessions s
            where s.start >= $1 and s.start < $2
              and ($3::text is null or s.instructor_id = $3)
            group by m
            "#,
        )
        .bind(start)
        .bind(next)
        .bind(filter)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

        let mut buckets: Vec<i64> = vec![0; 12];
        for (m, cnt) in rows {
            if (1..=12).contains(&m) {
                buckets[(m - 1) as usize] = cnt;
            }
        }
        buckets
            .into_iter()
            .enumerate()
            .map(|(i, sessions)| TrainingStatsMonthlyBucket {
                month: MONTH_ABBR[i].to_string(),
                sessions,
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(TrainingStatsBundle {
        sessions,
        total_hours,
        passed,
        failed,
        most_run_lesson,
        top_trainers,
        monthly_sessions,
        lesson_distribution,
    })
}

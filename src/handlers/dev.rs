use std::collections::HashMap;

use axum::{Json, extract::State};
use chrono::{Duration, Utc};
use serde::Serialize;

use crate::{errors::ApiError, repos::dev as dev_repo, state::AppState};

#[derive(Serialize)]
pub struct SeedResponse {
    ok: bool,
    users: usize,
    events: usize,
    event_positions: usize,
    certifications: usize,
    solo_certifications: usize,
    training_sessions: usize,
    training_appointments: usize,
    feedback: usize,
}

/// One controller in the seeded roster.
struct SeedUser {
    cid: i64,
    first: &'static str,
    last: &'static str,
    rating: &'static str,
    /// `HOME` / `VISITOR` / `NONE` (matches `org.memberships` CHECK).
    status: &'static str,
    oi: Option<&'static str>,
    roles: &'static [&'static str],
}

/// A full ZDC-flavored roster: senior staff, training team, event/web/facility
/// staff, home controllers across every rating, a couple of visitors, and one
/// off-roster applicant. CIDs 10000010–12 are the stable named anchors the rest
/// of the dev tooling logs in as.
const ROSTER: &[SeedUser] = &[
    // --- anchors (kept stable for dev login) ---
    SeedUser {
        cid: 10000010,
        first: "Dev",
        last: "Staff",
        rating: "C3",
        status: "HOME",
        oi: Some("DS"),
        roles: &["ATM", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000011,
        first: "Dev",
        last: "Student",
        rating: "S1",
        status: "HOME",
        oi: Some("DT"),
        roles: &["USER"],
    },
    SeedUser {
        cid: 10000012,
        first: "Dev",
        last: "Trainer",
        rating: "C1",
        status: "HOME",
        oi: Some("DR"),
        roles: &["INS", "STAFF", "USER"],
    },
    // --- senior / training / event / web / facility staff ---
    SeedUser {
        cid: 10000013,
        first: "Marcus",
        last: "Holt",
        rating: "C3",
        status: "HOME",
        oi: Some("MH"),
        roles: &["DATM", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000014,
        first: "Priya",
        last: "Nair",
        rating: "C1",
        status: "HOME",
        oi: Some("PN"),
        roles: &["TA", "INS", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000015,
        first: "Diego",
        last: "Alvarez",
        rating: "C1",
        status: "HOME",
        oi: Some("DA"),
        roles: &["ATA", "INS", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000016,
        first: "Sarah",
        last: "Chen",
        rating: "C3",
        status: "HOME",
        oi: Some("SC"),
        roles: &["EC", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000017,
        first: "Tom",
        last: "Becker",
        rating: "S3",
        status: "HOME",
        oi: Some("TB"),
        roles: &["AEC", "EVENT_STAFF", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000018,
        first: "Nadia",
        last: "Rahman",
        rating: "C1",
        status: "HOME",
        oi: Some("NR"),
        roles: &["WM", "WEB_TEAM", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000019,
        first: "Leo",
        last: "Fischer",
        rating: "S3",
        status: "HOME",
        oi: Some("LF"),
        roles: &["AWM", "WEB_TEAM", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000020,
        first: "Grace",
        last: "Okafor",
        rating: "C3",
        status: "HOME",
        oi: Some("GO"),
        roles: &["FE", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000021,
        first: "Ivan",
        last: "Petrov",
        rating: "C1",
        status: "HOME",
        oi: Some("IP"),
        roles: &["AFE", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000022,
        first: "Mia",
        last: "Lundgren",
        rating: "I1",
        status: "HOME",
        oi: Some("ML"),
        roles: &["INS", "MTR", "STAFF", "USER"],
    },
    SeedUser {
        cid: 10000023,
        first: "Owen",
        last: "Reyes",
        rating: "C1",
        status: "HOME",
        oi: Some("OR"),
        roles: &["MTR", "USER"],
    },
    SeedUser {
        cid: 10000024,
        first: "Hana",
        last: "Kim",
        rating: "C1",
        status: "HOME",
        oi: Some("HK"),
        roles: &["MTR", "USER"],
    },
    // --- home controllers across the rating ladder ---
    SeedUser {
        cid: 10000025,
        first: "Jack",
        last: "Sullivan",
        rating: "S3",
        status: "HOME",
        oi: Some("JS"),
        roles: &["USER"],
    },
    SeedUser {
        cid: 10000026,
        first: "Emma",
        last: "Novak",
        rating: "S2",
        status: "HOME",
        oi: Some("EN"),
        roles: &["USER"],
    },
    SeedUser {
        cid: 10000027,
        first: "Liam",
        last: "Walsh",
        rating: "S2",
        status: "HOME",
        oi: Some("LW"),
        roles: &["USER"],
    },
    SeedUser {
        cid: 10000028,
        first: "Zoe",
        last: "Martins",
        rating: "S1",
        status: "HOME",
        oi: Some("ZM"),
        roles: &["USER"],
    },
    SeedUser {
        cid: 10000029,
        first: "Noah",
        last: "Bauer",
        rating: "S1",
        status: "HOME",
        oi: Some("NB"),
        roles: &["USER"],
    },
    SeedUser {
        cid: 10000030,
        first: "Ava",
        last: "Costa",
        rating: "OBS",
        status: "HOME",
        oi: Some("AC"),
        roles: &["USER"],
    },
    // --- visitors ---
    SeedUser {
        cid: 10000031,
        first: "Ryan",
        last: "Doyle",
        rating: "C1",
        status: "VISITOR",
        oi: Some("RD"),
        roles: &["USER"],
    },
    SeedUser {
        cid: 10000032,
        first: "Chloe",
        last: "Weiss",
        rating: "S3",
        status: "VISITOR",
        oi: Some("CW"),
        roles: &["USER"],
    },
    // --- off-roster applicant ---
    SeedUser {
        cid: 10000033,
        first: "Sam",
        last: "Patel",
        rating: "OBS",
        status: "NONE",
        oi: None,
        roles: &["USER"],
    },
];

/// Instructors that generate training-session history (descending session counts
/// so the leaderboard has a clear ordering).
const INSTRUCTOR_CIDS: &[i64] = &[10000012, 10000014, 10000015, 10000022, 10000023, 10000024];

/// Students that receive training sessions / appointments.
const STUDENT_CIDS: &[i64] = &[
    10000011, 10000017, 10000019, 10000025, 10000026, 10000027, 10000028, 10000029, 10000030,
    10000032, 10000033,
];

/// Pilot feedback: (submitter_cid, target_cid, callsign, position, rating, comments, status).
const FEEDBACK: &[(i64, i64, &str, &str, i32, &str, &str)] = &[
    (
        10000025,
        10000012,
        "DAL1234",
        "DCA_TWR",
        5,
        "Excellent sequencing into DCA, very clear instructions.",
        "RELEASED",
    ),
    (
        10000026,
        10000014,
        "AAL88",
        "PCT_APP",
        5,
        "Smooth vectors and great energy management.",
        "RELEASED",
    ),
    (
        10000027,
        10000015,
        "UAL512",
        "DC_CTR",
        4,
        "Good handoffs, minor delay on descent clearance.",
        "RELEASED",
    ),
    (
        10000028,
        10000010,
        "JBU21",
        "DCA_GND",
        4,
        "Efficient ground movement during a busy push.",
        "PENDING",
    ),
    (
        10000029,
        10000013,
        "SWA909",
        "DCA_TWR",
        5,
        "Handled a go-around calmly and professionally.",
        "PENDING",
    ),
    (
        10000030,
        10000022,
        "N550SR",
        "IAD_TWR",
        3,
        "Helpful, though frequency was a little congested.",
        "PENDING",
    ),
    (
        10000032,
        10000023,
        "FDX1",
        "DC_CTR",
        5,
        "Top-tier center control, seamless coordination.",
        "RELEASED",
    ),
    (
        10000017,
        10000024,
        "RCH4001",
        "BWI_APP",
        4,
        "Clear approach control, appreciated the traffic calls.",
        "PENDING",
    ),
];

/// Certifications granted per rating tier: (certification_type_name, option).
fn certs_for(rating: &str) -> &'static [(&'static str, &'static str)] {
    match rating {
        "C3" | "C1" | "I1" | "I3" => &[
            ("GROUND", "CERTIFIED"),
            ("TOWER", "CERTIFIED"),
            ("APPROACH", "CERTIFIED"),
            ("CENTER", "TIER_1"),
        ],
        "S3" => &[
            ("GROUND", "CERTIFIED"),
            ("TOWER", "CERTIFIED"),
            ("APPROACH", "SOLO"),
        ],
        "S2" => &[("GROUND", "CERTIFIED"), ("TOWER", "SOLO")],
        "S1" => &[("GROUND", "CERTIFIED")],
        _ => &[],
    }
}

/// Representative solo position for a certification type.
fn solo_position(cert_type: &str) -> &'static str {
    match cert_type {
        "GROUND" => "DCA_GND",
        "TOWER" => "DCA_TWR",
        "APPROACH" => "PCT_APP",
        "CENTER" => "DC_CTR",
        _ => "DCA_TWR",
    }
}

pub async fn seed_data(State(state): State<AppState>) -> Result<Json<SeedResponse>, ApiError> {
    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let now = Utc::now();

    // --- users + roles ---
    let mut ids: HashMap<i64, String> = HashMap::new();
    for u in ROSTER {
        let user_id = dev_repo::upsert_user(
            &mut tx,
            &format!("seed-user-{}", u.cid),
            u.cid,
            &format!("dev-{}@example.invalid", u.cid),
            &format!("{} {}", u.first, u.last),
            Some(u.first),
            Some(u.last),
            Some("ZDC"),
            Some(u.rating),
            Some("USA"),
            u.status,
            u.oi,
        )
        .await?;
        for &role in u.roles {
            dev_repo::grant_user_role(&mut tx, &user_id, role).await?;
        }
        ids.insert(u.cid, user_id);
    }

    let staff_id = ids.get(&10000010).ok_or(ApiError::Internal)?.clone();
    let student_id = ids.get(&10000011).ok_or(ApiError::Internal)?.clone();
    let trainer_id = ids.get(&10000012).ok_or(ApiError::Internal)?.clone();

    // --- certifications + solo endorsements ---
    let mut cert_types: HashMap<&str, String> = HashMap::new();
    for name in ["GROUND", "TOWER", "APPROACH", "CENTER"] {
        cert_types.insert(name, dev_repo::cert_type_id(&mut tx, name).await?);
    }

    let mut certifications = 0usize;
    let mut solo_certifications = 0usize;
    for u in ROSTER {
        let Some(uid) = ids.get(&u.cid) else { continue };
        for &(type_name, option) in certs_for(u.rating) {
            let Some(type_id) = cert_types.get(type_name) else {
                continue;
            };
            dev_repo::upsert_certification(&mut tx, uid, type_id, option).await?;
            certifications += 1;
            if option == "SOLO" {
                dev_repo::upsert_solo_cert(
                    &mut tx,
                    &format!("seed-solo-{}-{}", u.cid, type_name),
                    uid,
                    type_id,
                    solo_position(type_name),
                    now + Duration::days(21),
                )
                .await?;
                solo_certifications += 1;
            }
        }
    }

    // --- events ---
    dev_repo::upsert_event(
        &mut tx,
        "seed-event-1",
        "Seeded Dev Event",
        "HOME",
        "SCHEDULED",
        true,
        now + Duration::days(1),
        now + Duration::days(1) + Duration::hours(4),
        &staff_id,
    )
    .await?;
    dev_repo::upsert_event(
        &mut tx,
        "seed-event-2",
        "vZDC Capital Classic (Past)",
        "STANDARD",
        "ARCHIVED",
        true,
        now - Duration::days(14),
        now - Duration::days(14) + Duration::hours(4),
        &staff_id,
    )
    .await?;
    dev_repo::upsert_event(
        &mut tx,
        "seed-event-3",
        "Friday Night Ops",
        "FRIDAY_NIGHT_OPERATIONS",
        "DRAFT",
        false,
        now + Duration::days(9),
        now + Duration::days(9) + Duration::hours(3),
        &staff_id,
    )
    .await?;

    // --- event positions on the upcoming published event ---
    let positions: &[(&str, &str, Option<i64>, &str)] = &[
        ("seed-pos-1", "DCA_DEL", Some(10000028), "ASSIGNED"),
        ("seed-pos-2", "DCA_GND", Some(10000026), "ASSIGNED"),
        ("seed-pos-3", "DCA_TWR", Some(10000017), "PUBLISHED"),
        ("seed-pos-4", "PCT_APP", Some(10000015), "ASSIGNED"),
        ("seed-pos-5", "DC_CTR", None, "OPEN"),
    ];
    let mut event_positions = 0usize;
    for (pos_id, callsign, cid, status) in positions {
        let user_id = cid.and_then(|c| ids.get(&c)).map(String::as_str);
        let assigned_slot = user_id.map(|_| 1);
        dev_repo::upsert_event_position(
            &mut tx,
            pos_id,
            "seed-event-1",
            callsign,
            user_id,
            1,
            assigned_slot,
            status,
        )
        .await?;
        event_positions += 1;
    }

    // --- training session history (leaderboard) ---
    let mut training_sessions = 0usize;
    for (t, inst_cid) in INSTRUCTOR_CIDS.iter().enumerate() {
        let Some(instructor_id) = ids.get(inst_cid) else {
            continue;
        };
        let session_count = 14usize.saturating_sub(t * 2); // 14, 12, 10, 8, 6, 4
        for s in 0..session_count {
            let stu_cid = STUDENT_CIDS[(t + s) % STUDENT_CIDS.len()];
            let Some(student) = ids.get(&stu_cid) else {
                continue;
            };
            let days_ago = t as i64 + s as i64 * 2 + 1;
            let duration_min = 60 + (s % 3) as i64 * 30; // 60 / 90 / 120
            let start = now - Duration::days(days_ago) - Duration::hours(2);
            let end = start + Duration::minutes(duration_min);
            dev_repo::upsert_training_session(
                &mut tx,
                &format!("seed-session-{t}-{s}"),
                student,
                instructor_id,
                start,
                end,
                "Seeded training session — solid progress.",
            )
            .await?;
            training_sessions += 1;
        }
    }

    // --- upcoming training appointments ---
    let environments = ["DCA", "IAD", "BWI", "PCT", "ZDC"];
    let mut training_appointments = 0usize;
    for i in 0..6usize {
        let stu_cid = STUDENT_CIDS[i % STUDENT_CIDS.len()];
        let inst_cid = INSTRUCTOR_CIDS[i % INSTRUCTOR_CIDS.len()];
        let (Some(student), Some(trainer)) = (ids.get(&stu_cid), ids.get(&inst_cid)) else {
            continue;
        };
        dev_repo::upsert_training_appointment(
            &mut tx,
            &format!("seed-appt-{i}"),
            student,
            trainer,
            now + Duration::days(i as i64 + 1) + Duration::hours(1),
            Some(environments[i % environments.len()]),
            "Seeded upcoming appointment.",
        )
        .await?;
        training_appointments += 1;
    }

    // --- pilot feedback ---
    let mut feedback = 0usize;
    for (i, (submitter, target, callsign, position, rating, comments, status)) in
        FEEDBACK.iter().enumerate()
    {
        let (Some(submitter_id), Some(target_id)) = (ids.get(submitter), ids.get(target)) else {
            continue;
        };
        dev_repo::upsert_feedback(
            &mut tx,
            &format!("seed-feedback-{i}"),
            submitter_id,
            target_id,
            callsign,
            position,
            *rating,
            comments,
            status,
        )
        .await?;
        feedback += 1;
    }

    // --- training assignment / requests / release (existing anchor records) ---
    dev_repo::upsert_event_tmi(&mut tx).await?;
    dev_repo::upsert_ops_plan_file(&mut tx, &staff_id).await?;

    let assignment_id =
        dev_repo::upsert_training_assignment(&mut tx, &student_id, &staff_id).await?;
    dev_repo::insert_assignment_other_trainer(&mut tx, &assignment_id, &trainer_id).await?;
    dev_repo::upsert_assignment_request(&mut tx, &student_id).await?;
    dev_repo::insert_assignment_request_interested_trainer(&mut tx, &trainer_id).await?;
    dev_repo::upsert_trainer_release_request(&mut tx, &student_id).await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(Json(SeedResponse {
        ok: true,
        users: ROSTER.len(),
        events: 3,
        event_positions,
        certifications,
        solo_certifications,
        training_sessions,
        training_appointments,
        feedback,
    }))
}

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

pub fn checksum(payload: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload.as_ref());
    format!("{:x}", hasher.finalize())
}

pub fn normalize_rating(value: &str) -> Result<i32> {
    match value.trim().to_uppercase().as_str() {
        "SUS" => Ok(0),
        "OBS" => Ok(1),
        "S1" => Ok(2),
        "S2" => Ok(3),
        "S3" => Ok(4),
        "C1" => Ok(5),
        "C2" => Ok(6),
        "C3" => Ok(7),
        "I1" => Ok(8),
        "I2" => Ok(9),
        "I3" => Ok(10),
        "SUP" => Ok(11),
        "ADM" => Ok(12),
        other => bail!("unknown rating value `{other}`"),
    }
}

pub fn normalize_controller_status(value: &str) -> Result<&'static str> {
    match value.trim().to_uppercase().as_str() {
        "HOME" => Ok("HOME"),
        "VISITOR" => Ok("VISITOR"),
        "NONE" => Ok("NONE"),
        other => bail!("unknown controller status `{other}`"),
    }
}

pub fn normalize_role(role_name: &str) -> Option<&'static str> {
    match role_name.trim().to_uppercase().as_str() {
        "USER" => Some("CONTROLLER"),
        "STAFF" => Some("STAFF"),
        "INS" => Some("INSTRUCTOR"),
        "MTR" => Some("MENTOR"),
        "EVENT_STAFF" => Some("EVENT_STAFF"),
        "WEB_TEAM" => Some("WEB_TEAM"),
        _ => None,
    }
}

pub fn normalize_staff_position(position_name: &str) -> Option<&'static str> {
    match position_name.trim().to_uppercase().as_str() {
        "ATM" => Some("ATM"),
        "DATM" => Some("DATM"),
        "TA" => Some("TA"),
        "EC" => Some("EC"),
        "FE" => Some("FE"),
        "WM" => Some("WM"),
        "ATA" => Some("ATA"),
        "AWM" => Some("AWM"),
        "AEC" => Some("AEC"),
        "AFE" => Some("AFE"),
        "INS" => Some("INS"),
        "MTR" => Some("MTR"),
        _ => None,
    }
}

pub fn normalize_event_type(value: &str) -> Result<&'static str> {
    match value.trim().to_uppercase().as_str() {
        "HOME" => Ok("HOME"),
        "SUPPORT_REQUIRED" => Ok("SUPPORT_REQUIRED"),
        "SUPPORT_OPTIONAL" => Ok("SUPPORT_OPTIONAL"),
        "GROUP_FLIGHT" => Ok("GROUP_FLIGHT"),
        "FRIDAY_NIGHT_OPERATIONS" => Ok("FRIDAY_NIGHT_OPERATIONS"),
        "SATURDAY_NIGHT_OPERATIONS" => Ok("SATURDAY_NIGHT_OPERATIONS"),
        "TRAINING" => Ok("TRAINING"),
        "STANDARD" => Ok("HOME"),
        other => bail!("unknown event type `{other}`"),
    }
}

pub fn normalize_tmi_category(value: &str) -> Result<&'static str> {
    match value.trim().to_uppercase().as_str() {
        "LOCAL" => Ok("LOCAL"),
        "TERMINAL" | "APP" => Ok("TERMINAL"),
        "ENROUTE" | "CTR" => Ok("ENROUTE"),
        other => bail!("unknown TMI category `{other}`"),
    }
}

pub fn normalize_certification_option(value: &str) -> Result<&'static str> {
    match value.trim().to_uppercase().as_str() {
        "NONE" => Ok("NONE"),
        "UNRESTRICTED" => Ok("UNRESTRICTED"),
        "DEL" => Ok("DEL"),
        "GND" => Ok("GND"),
        "TWR" => Ok("TWR"),
        "APP" => Ok("APP"),
        "CTR" => Ok("CTR"),
        "TIER_1" => Ok("TIER_1"),
        "CERTIFIED" => Ok("CERTIFIED"),
        "SOLO" => Ok("SOLO"),
        other => bail!("unknown certification option `{other}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_certification_option, normalize_controller_status, normalize_event_type,
        normalize_rating, normalize_role, normalize_staff_position, normalize_tmi_category,
    };

    #[test]
    fn normalizes_known_ratings() {
        assert_eq!(normalize_rating("OBS").unwrap(), 1);
        assert_eq!(normalize_rating("C1").unwrap(), 5);
        assert_eq!(normalize_rating("ADM").unwrap(), 12);
    }

    #[test]
    fn rejects_unknown_rating() {
        assert!(normalize_rating("XYZ").is_err());
    }

    #[test]
    fn normalizes_roles_and_staff_positions() {
        assert_eq!(normalize_role("USER"), Some("CONTROLLER"));
        assert_eq!(normalize_role("WEB_TEAM"), Some("WEB_TEAM"));
        assert_eq!(normalize_staff_position("ta"), Some("TA"));
    }

    #[test]
    fn normalizes_controller_status_and_events() {
        assert_eq!(normalize_controller_status("home").unwrap(), "HOME");
        assert_eq!(normalize_event_type("STANDARD").unwrap(), "HOME");
        assert_eq!(normalize_tmi_category("ctr").unwrap(), "ENROUTE");
    }

    #[test]
    fn normalizes_certification_options() {
        assert_eq!(
            normalize_certification_option("unrestricted").unwrap(),
            "UNRESTRICTED"
        );
        assert_eq!(normalize_certification_option("solo").unwrap(), "SOLO");
    }
}

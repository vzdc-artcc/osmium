use std::{fmt, str::FromStr};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Persona {
    Staff,
    Student,
    Trainer,
    Admin,
}

impl Persona {
    pub fn as_str(self) -> &'static str {
        match self {
            Persona::Staff => "staff",
            Persona::Student => "student",
            Persona::Trainer => "trainer",
            Persona::Admin => "admin",
        }
    }

    pub fn default_cid(self) -> Option<i64> {
        match self {
            Persona::Staff => Some(10000010),
            Persona::Student => Some(10000011),
            Persona::Trainer => Some(10000012),
            Persona::Admin => None,
        }
    }

    pub fn env_key(self) -> &'static str {
        match self {
            Persona::Staff => "API_LOAD_BEARER_STAFF",
            Persona::Student => "API_LOAD_BEARER_STUDENT",
            Persona::Trainer => "API_LOAD_BEARER_TRAINER",
            Persona::Admin => "API_LOAD_BEARER_ADMIN",
        }
    }
}

impl fmt::Display for Persona {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Persona {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "staff" => Ok(Persona::Staff),
            "student" => Ok(Persona::Student),
            "trainer" => Ok(Persona::Trainer),
            "admin" => Ok(Persona::Admin),
            other => Err(format!("unsupported persona '{other}'")),
        }
    }
}

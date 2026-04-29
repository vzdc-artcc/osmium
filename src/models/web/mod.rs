use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct PublicationCategory {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub sort_order: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Publication {
    pub id: String,
    pub category_id: String,
    pub category_key: String,
    pub category_name: String,
    pub title: String,
    pub description: Option<String>,
    pub effective_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub file_id: String,
    pub cdn_url: String,
    pub file_filename: String,
    pub file_content_type: String,
    pub file_size_bytes: i64,
    pub is_public: bool,
    pub sort_order: i32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePublicationRequest {
    pub category_id: String,
    pub title: String,
    pub description: Option<String>,
    pub effective_at: chrono::DateTime<chrono::Utc>,
    pub file_id: String,
    pub is_public: bool,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub sort_order: Option<i32>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdatePublicationRequest {
    pub category_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub effective_at: Option<chrono::DateTime<chrono::Utc>>,
    pub file_id: Option<String>,
    pub is_public: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub sort_order: Option<i32>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePublicationCategoryRequest {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdatePublicationCategoryRequest {
    pub key: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub sort_order: Option<i32>,
}

fn deserialize_optional_i32<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptionalI32Visitor;

    impl<'de> Visitor<'de> for OptionalI32Visitor {
        type Value = Option<i32>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an integer, a numeric string, an empty string, or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserialize_optional_i32(deserializer)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let parsed = i32::try_from(value)
                .map_err(|_| E::invalid_value(de::Unexpected::Signed(value), &self))?;
            Ok(Some(parsed))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let parsed = i32::try_from(value)
                .map_err(|_| E::invalid_value(de::Unexpected::Unsigned(value), &self))?;
            Ok(Some(parsed))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }

            let parsed = trimmed.parse::<i32>().map_err(|_| {
                E::invalid_value(de::Unexpected::Str(value), &"an integer or empty string")
            })?;
            Ok(Some(parsed))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_any(OptionalI32Visitor)
}

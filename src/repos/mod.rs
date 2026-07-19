pub mod access;
pub mod api_keys;
pub mod audit;
pub mod broadcasts;
pub mod email_branding;
pub mod events;
pub mod feedback;
pub mod files;
pub mod incidents;
pub mod integrations;
pub mod org;
pub mod publications;
pub mod stats;
pub mod training;
pub mod training_admin;
pub mod users;
pub mod welcome_messages;

/// Maps common Postgres constraint-violation error codes (foreign key, unique,
/// check, invalid text representation, not-null) to `ApiError::BadRequest`, and
/// anything else to `ApiError::Internal`.
pub fn map_constraint_error(error: sqlx::Error) -> crate::errors::ApiError {
    match &error {
        sqlx::Error::Database(database_error) => match database_error.code().as_deref() {
            Some("23503") | Some("23505") | Some("23514") | Some("22P02") | Some("23502") => {
                crate::errors::ApiError::BadRequest
            }
            _ => crate::errors::ApiError::Internal,
        },
        _ => crate::errors::ApiError::Internal,
    }
}

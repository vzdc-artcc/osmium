use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CurrentUser {
    pub id: String,
    pub cid: i64,
    pub email: String,
    pub display_name: String,
    pub rating: Option<String>,
    pub primary_role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CurrentServiceAccount {
    pub id: String,
    pub key: String,
    pub name: String,
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use shared::types::Role;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    // subject -- the auth_user id
    pub sub: Uuid,
    // the actual patient or staff entity id
    pub entity_id: Uuid,
    // "staff" or "patient"
    pub entity_type: String,
    // e.g. ["physician"] or ["admin"]
    pub roles: Vec<Role>,
    // department if staff, None if patient
    pub department: Option<String>,
    // issued at (unix timestamp)
    pub iat: i64,
    // expiry (unix timestamp)
    pub exp: i64,
}

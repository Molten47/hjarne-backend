use chrono::Utc;
use jsonwebtoken::{
    decode, encode,
    Algorithm, DecodingKey, EncodingKey,
    Header, Validation,
};
use shared::errors::{AppError, AppResult};
use uuid::Uuid;
use crate::claims::Claims;
use shared::types::Role;

pub fn create_access_token(
    user_id: Uuid,
    entity_id: Uuid,
    entity_type: String,
    roles: Vec<Role>,
    department: Option<String>,
    private_key_pem: &str,
    expiry_seconds: i64,
) -> AppResult<String> {
    let now = Utc::now().timestamp();

    let claims = Claims {
        sub: user_id,
        entity_id,
        entity_type,
        roles,
        department,
        iat: now,
        exp: now + expiry_seconds,
    };

    let key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
        .map_err(|e| AppError::Internal(format!("invalid private key: {e}")))?;

    encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|e| AppError::Internal(format!("failed to create token: {e}")))
}

pub fn verify_access_token(token: &str, public_key_pem: &str) -> AppResult<Claims> {
    let key = DecodingKey::from_rsa_pem(public_key_pem.as_bytes())
        .map_err(|e| AppError::Internal(format!("invalid public key: {e}")))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = true;

    decode::<Claims>(token, &key, &validation)
        .map(|data| data.claims)
        .map_err(|e| {
            tracing::warn!("token verification failed: {e}");
            AppError::Unauthorized(format!("invalid or expired token: {e}"))
        })
}

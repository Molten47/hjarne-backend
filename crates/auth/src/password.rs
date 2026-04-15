use shared::errors::{AppError, AppResult};
use tracing::instrument;

#[instrument(skip(password))]
pub fn hash_password(password: &str) -> AppResult<String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| AppError::Internal(format!("failed to hash password: {e}")))
}

#[instrument(skip(password, hash))]
pub fn verify_password(password: &str, hash: &str) -> AppResult<bool> {
    bcrypt::verify(password, hash)
        .map_err(|e| AppError::Internal(format!("failed to verify password: {e}")))
}
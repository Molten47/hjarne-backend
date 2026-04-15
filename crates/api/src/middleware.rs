use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
    Extension,
};
use auth::verify_access_token;
use shared::errors::AppError;
use crate::state::AppState;
use auth::Claims;


// this extractor is injected into protected handlers
#[derive(Clone, Debug)]
pub struct AuthUser(pub Claims);

// JWT auth middleware
// extracts Bearer token, verifies it, injects Claims into request extensions
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Unauthorized("missing authorization header".to_string()))?;

    let claims = verify_access_token(token, &state.config.jwt_public_key)?;

    request.extensions_mut().insert(AuthUser(claims));
    Ok(next.run(request).await)
}
pub async fn require_admin(
    State(_state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    use shared::types::Role;
    if !auth_user.0.roles.contains(&Role::Admin) {
        return Err(AppError::Forbidden("admin access required".to_string()));
    }
    Ok(next.run(request).await)
}
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use shared::{
    errors::{AppError, AppResult},
    response::ApiResponse,
    types::Role,
};
use uuid::Uuid;
use validator::Validate;
use chrono::Utc;
use sha2::{Sha256, Digest};

use crate::state::AppState;
use auth::{create_access_token, verify_password, hash_password};
use axum::Extension;
use crate::middleware::AuthUser;

// ── request / response shapes ─────────────────────────────────────────────────

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email(message = "invalid email address"))]
    pub email: String,
    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ChangePasswordRequest {
    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub must_change_password: bool,
    pub user: UserSummary,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

#[derive(Debug, Serialize)]
pub struct UserSummary {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub entity_type: String,
    pub email: String,
    pub roles: Vec<Role>,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub user_id: Uuid,
    pub entity_id: Uuid,
    pub entity_type: String,
    pub roles: Vec<Role>,
    pub department: Option<String>,
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn generate_refresh_token() -> String {
    use std::fmt::Write;
    let id1 = Uuid::new_v4().as_simple().to_string();
    let id2 = Uuid::new_v4().as_simple().to_string();
    format!("{}{}", id1, id2)
}

fn parse_role(role: &str) -> Role {
    match role {
        "admin"      => Role::Admin,
        "desk"       => Role::Desk,
        "physician"  => Role::Physician,
        "surgeon"    => Role::Surgeon,
        "nurse"      => Role::Nurse,
        "pharmacist" => Role::Pharmacist,
        _            => Role::Patient,
    }
}

// ── login ─────────────────────────────────────────────────────────────────────

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> AppResult<Json<ApiResponse<LoginResponse>>> {
    payload.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let user = sqlx::query!(
        r#"
        SELECT id, entity_id, entity_type, email,
               password_hash, is_active, must_change_password
        FROM auth_users
        WHERE email = $1
        "#,
        payload.email
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Unauthorized("invalid email or password".to_string()))?;

    if !user.is_active {
        return Err(AppError::Unauthorized("account is deactivated".to_string()));
    }

    if !verify_password(&payload.password, &user.password_hash)? {
        return Err(AppError::Unauthorized("invalid email or password".to_string()));
    }

    let roles = if user.entity_type == "staff" {
        let staff = sqlx::query!(
            "SELECT role FROM staff WHERE id = $1", user.entity_id
        )
        .fetch_optional(&state.db)
        .await?;
        match staff {
            Some(s) => vec![parse_role(&s.role)],
            None => vec![],
        }
    } else {
        vec![Role::Patient]
    };

    let department = if user.entity_type == "staff" {
        sqlx::query_scalar!(
            "SELECT department FROM staff WHERE id = $1", user.entity_id
        )
        .fetch_optional(&state.db)
        .await?
        .flatten()
    } else {
        None
    };

    // RS256 access token
    let access_token = create_access_token(
        user.id,
        user.entity_id,
        user.entity_type.clone(),
        roles.clone(),
        department,
        &state.config.jwt_private_key,
        state.config.jwt_access_expiry_seconds,
    )?;

    // refresh token — opaque, stored hashed, with family_id
    let raw_refresh   = generate_refresh_token();
    let token_hash    = hash_token(&raw_refresh);
    let family_id     = Uuid::new_v4();
    let refresh_expiry = Utc::now()
        + chrono::Duration::days(state.config.jwt_refresh_expiry_days);

    sqlx::query!(
        r#"
        INSERT INTO refresh_tokens
            (user_id, token_hash, family_id, expires_at, revoked)
        VALUES ($1, $2, $3, $4, FALSE)
        "#,
        user.id,
        token_hash,
        family_id,
        refresh_expiry,
    )
    .execute(&state.db)
    .await?;

    // cache family → current token hash for O(1) reuse detection
    let cache_key = cache::keys::refresh_token_family(family_id);
    let mut cache = state.cache.clone();
    cache.store_refresh_token(
        &cache_key,
        &token_hash,
        (state.config.jwt_refresh_expiry_days * 86400) as u64,
    ).await.ok();

    sqlx::query!(
        "UPDATE auth_users SET last_login = NOW() WHERE id = $1", user.id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(LoginResponse {
        access_token,
        refresh_token: raw_refresh,
        token_type: "Bearer".to_string(),
        expires_in: state.config.jwt_access_expiry_seconds,
        must_change_password: user.must_change_password,
        user: UserSummary {
            id: user.id,
            entity_id: user.entity_id,
            entity_type: user.entity_type,
            email: user.email,
            roles,
        },
    })))
}

// ── refresh ───────────────────────────────────────────────────────────────────

pub async fn refresh(
    State(state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> AppResult<Json<ApiResponse<RefreshResponse>>> {
    let incoming_hash = hash_token(&payload.refresh_token);

    // look up token in DB
    let token_row = sqlx::query!(
        r#"
        SELECT id, user_id, family_id, expires_at, revoked
        FROM refresh_tokens
        WHERE token_hash = $1
        "#,
        incoming_hash,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Unauthorized("invalid refresh token".to_string()))?;

    // expired?
    if token_row.expires_at < Utc::now() {
        return Err(AppError::Unauthorized("refresh token expired".to_string()));
    }

    // reuse detected — token already revoked but someone is presenting it
    if token_row.revoked {
        // kill entire family — assume compromise
        sqlx::query!(
            "UPDATE refresh_tokens SET revoked = TRUE WHERE family_id = $1",
            token_row.family_id
        )
        .execute(&state.db)
        .await?;

        // purge family from Redis
        let cache_key = cache::keys::refresh_token_family(token_row.family_id);
        let mut cache = state.cache.clone();
        cache.revoke_refresh_token(&cache_key).await.ok();

        tracing::warn!(
            family_id = %token_row.family_id,
            user_id   = %token_row.user_id,
            "refresh token reuse detected — entire family revoked"
        );

        return Err(AppError::Unauthorized(
            "token reuse detected — all sessions revoked".to_string()
        ));
    }

    // valid — rotate: revoke old, issue new in same family
    sqlx::query!(
        "UPDATE refresh_tokens SET revoked = TRUE WHERE id = $1",
        token_row.id
    )
    .execute(&state.db)
    .await?;

    let raw_refresh   = generate_refresh_token();
    let new_hash      = hash_token(&raw_refresh);
    let refresh_expiry = Utc::now()
        + chrono::Duration::days(state.config.jwt_refresh_expiry_days);

    sqlx::query!(
        r#"
        INSERT INTO refresh_tokens
            (user_id, token_hash, family_id, expires_at, revoked)
        VALUES ($1, $2, $3, $4, FALSE)
        "#,
        token_row.user_id,
        new_hash,
        token_row.family_id,   // same family
        refresh_expiry,
    )
    .execute(&state.db)
    .await?;

    // update Redis family → new hash
    let cache_key = cache::keys::refresh_token_family(token_row.family_id);
    let mut cache = state.cache.clone();
    cache.store_refresh_token(
        &cache_key,
        &new_hash,
        (state.config.jwt_refresh_expiry_days * 86400) as u64,
    ).await.ok();

    // fetch user to rebuild access token
let user = sqlx::query!(
    r#"
    SELECT au.id, au.entity_id, au.entity_type,
           s.role AS "role?: String", s.department AS "department?: String"
    FROM auth_users au
    LEFT JOIN staff s ON s.id = au.entity_id
    WHERE au.id = $1
    "#,
    token_row.user_id
    )
    .fetch_one(&state.db)
    .await?;

    let roles = match user.role.as_ref().map(|s| s.as_str()) {
        Some(r) => vec![parse_role(r)],
        None    => vec![Role::Patient],
    };

    let access_token = create_access_token(
        user.id,
        user.entity_id,
        user.entity_type.clone(),
        roles,
        user.department,
        &state.config.jwt_private_key,
        state.config.jwt_access_expiry_seconds,
    )?;

    Ok(Json(ApiResponse::success(RefreshResponse {
        access_token,
        refresh_token: raw_refresh,
        expires_in: state.config.jwt_access_expiry_seconds,
    })))
}

// ── change password ───────────────────────────────────────────────────────────

pub async fn change_password(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(payload): Json<ChangePasswordRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    payload.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let new_hash = hash_password(&payload.new_password)?;

    sqlx::query!(
        r#"
        UPDATE auth_users
        SET password_hash = $1, must_change_password = FALSE
        WHERE id = $2
        "#,
        new_hash,
        auth_user.0.sub,
    )
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "message": "password changed successfully"
    }))))
}

// ── me ────────────────────────────────────────────────────────────────────────

pub async fn me(
    Extension(auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<MeResponse>>> {
    let claims = auth_user.0;
    Ok(Json(ApiResponse::success(MeResponse {
        user_id:     claims.sub,
        entity_id:   claims.entity_id,
        entity_type: claims.entity_type,
        roles:       claims.roles,
        department:  claims.department,
    })))
}

// ── logout ────────────────────────────────────────────────────────────────────

pub async fn logout(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let user_id = auth_user.0.sub;

    // revoke all refresh token families for this user
    sqlx::query!(
        "UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1 AND revoked = FALSE",
        user_id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "message": "logged out successfully"
    }))))
}
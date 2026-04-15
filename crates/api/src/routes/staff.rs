use axum::{extract::{Path, Query, State}, Extension, Json};
use serde::{Deserialize, Serialize};
use shared::{
    errors::{AppError, AppResult},
    response::ApiResponse,
};
use uuid::Uuid;
use validator::Validate;
use chrono::{DateTime, Utc};

use crate::{middleware::AuthUser, state::AppState};
use auth::hash_password;

#[derive(Debug, Deserialize)]
pub struct StaffListQuery {
    pub role: Option<String>,
    pub department: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StaffSummary {
    pub id: Uuid,
    pub staff_code: String,
    pub first_name: String,
    pub last_name: String,
    pub role: String,
    pub department: Option<String>,
    pub specialization: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub has_auth: bool,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateStaffRequest {
    #[validate(length(min = 1, max = 100))]
    pub first_name: String,
    #[validate(length(min = 1, max = 100))]
    pub last_name: String,
    pub role: String,
    pub department: Option<String>,
    pub specialization: Option<String>,
    pub license_number: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateStaffAuthRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
}

pub async fn list_staff(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    Query(params): Query<StaffListQuery>,
) -> AppResult<Json<ApiResponse<Vec<StaffSummary>>>> {
    let limit = params.limit.unwrap_or(50).min(100);

    let staff = sqlx::query_as!(
        StaffSummary,
        r#"
        SELECT
            s.id, s.staff_code, s.first_name, s.last_name, s.role,
            s.department, s.specialization, s.is_active, s.created_at,
            (au.id IS NOT NULL) AS "has_auth!"
        FROM staff s
        LEFT JOIN auth_users au ON au.entity_id = s.id AND au.entity_type = 'staff'
        WHERE ($1::text IS NULL OR s.role = $1)
          AND ($2::text IS NULL OR s.department = $2)
          AND s.is_active = TRUE
        ORDER BY s.last_name, s.first_name
        LIMIT $3
        "#,
        params.role,
        params.department,
        limit
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(staff)))
}

pub async fn create_staff(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    Json(payload): Json<CreateStaffRequest>,
) -> AppResult<Json<ApiResponse<StaffSummary>>> {
    payload.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM staff")
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0);

    let prefix = match payload.role.as_str() {
        "physician"  => "PHY",
        "surgeon"    => "SRG",
        "nurse"      => "NRS",
        "pharmacist" => "PHM",
        "desk"       => "DSK",
        "admin"      => "ADM",
        _            => "STF",
    };
    let staff_code = format!("{}-{:04}", prefix, count + 1);

    let staff = sqlx::query_as!(
        StaffSummary,
        r#"
        INSERT INTO staff (
            staff_code, first_name, last_name, role,
            department, specialization, license_number
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, staff_code, first_name, last_name, role,
                  department, specialization, is_active, created_at,
                  FALSE AS "has_auth!"
        "#,
        staff_code,
        payload.first_name,
        payload.last_name,
        payload.role,
        payload.department,
        payload.specialization,
        payload.license_number
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(staff)))
}

pub async fn create_staff_auth(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    Path(staff_id): Path<Uuid>,
    Json(payload): Json<CreateStaffAuthRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    payload.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let exists = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM staff WHERE id = $1",
        staff_id
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0) > 0;

    if !exists {
        return Err(AppError::NotFound(format!("staff {staff_id} not found")));
    }

    let email_taken = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM auth_users WHERE email = $1",
        payload.email
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0) > 0;

    if email_taken {
        return Err(AppError::Conflict("email already registered".to_string()));
    }

    let password_hash = hash_password(&payload.password)?;

    sqlx::query!(
        r#"
        INSERT INTO auth_users (entity_id, entity_type, email, password_hash, must_change_password)
        VALUES ($1, 'staff', $2, $3, TRUE)
        "#,
        staff_id,
        payload.email,
        password_hash
    )
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "message": "credentials created - staff must change password on first login"
    }))))
}
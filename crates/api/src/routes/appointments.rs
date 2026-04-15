use axum::{extract::{Path, Query, State}, Extension, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::{
    errors::{AppError, AppResult},
    response::ApiResponse,
};
use uuid::Uuid;
use validator::Validate;

use crate::{middleware::AuthUser, state::AppState};

#[derive(Debug, Deserialize)]
pub struct AppointmentListQuery {
    pub physician_id: Option<Uuid>,
    pub patient_id: Option<Uuid>,
    pub status: Option<String>,
    pub date: Option<String>,
    pub limit: Option<i64>,
    pub cursor_time: Option<DateTime<Utc>>,
    pub cursor_id: Option<Uuid>,
}
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct AppointmentRow {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub physician_id: Option<Uuid>,
    pub department: String,
    pub appointment_type: String,
    pub status: String,
    pub scheduled_at: DateTime<Utc>,
    pub duration_minutes: i32,
    pub reason: Option<String>,
    pub channel: Option<String>,
    pub daily_room_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateAppointmentRequest {
    pub patient_id: Uuid,
    pub physician_id: Option<Uuid>,
    pub department: String,
    pub appointment_type: String,
    pub scheduled_at: DateTime<Utc>,
    pub duration_minutes: Option<i32>,
    pub reason: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
    pub notes: Option<String>,
}

pub async fn list_appointments(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<AppointmentListQuery>,
) -> AppResult<Json<ApiResponse<Vec<AppointmentRow>>>> {
    let limit = params.limit.unwrap_or(50).min(100);

    let is_admin = auth_user.0.roles.iter().any(|r| {
        matches!(r, shared::types::Role::Admin | shared::types::Role::Desk)
    });

    // Non-admins can only see their own appointments
    let effective_physician_id = if is_admin {
        params.physician_id
    } else {
        Some(auth_user.0.entity_id)
    };

    let mut appointments = sqlx::query_as!(
        AppointmentRow,
        r#"
        SELECT id, patient_id, physician_id, department,
               appointment_type, status, scheduled_at,
               duration_minutes, reason, channel, daily_room_url, created_at
        FROM appointments
        WHERE ($1::uuid IS NULL OR physician_id = $1)
          AND ($2::uuid IS NULL OR patient_id = $2)
          AND ($3::text IS NULL OR status = $3)
          AND (
            $4::timestamptz IS NULL OR $5::uuid IS NULL
            OR (scheduled_at, id) < ($4, $5)
          )
        ORDER BY scheduled_at DESC, id DESC
        LIMIT $6
        "#,
        effective_physician_id as Option<Uuid>,
        params.patient_id as Option<Uuid>,
        params.status,
        params.cursor_time as Option<DateTime<Utc>>,
        params.cursor_id as Option<Uuid>,
        limit + 1
    )
    .fetch_all(&state.db)
    .await?;

    let next_cursor = if appointments.len() as i64 > limit {
        appointments.pop();
        appointments.last().map(|a| {
            format!("{},{}", a.scheduled_at.to_rfc3339(), a.id)
        })
    } else {
        None
    };

    let meta = shared::response::Meta {
        page: None,
        total: None,
        next_cursor,
        request_id: None,
    };

    Ok(Json(ApiResponse::success_with_meta(appointments, meta)))
}

pub async fn create_appointment(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(payload): Json<CreateAppointmentRequest>,
) -> AppResult<Json<ApiResponse<AppointmentRow>>> {
    let booked_by = auth_user.0.entity_id;

    let appointment = sqlx::query_as!(
        AppointmentRow,
        r#"
        INSERT INTO appointments (
            patient_id, physician_id, booked_by, department,
            appointment_type, scheduled_at, duration_minutes,
            reason, channel
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
  RETURNING id, patient_id, physician_id, department,
                  appointment_type, status, scheduled_at,
                  duration_minutes, reason, channel, daily_room_url, created_at
        "#,
        payload.patient_id,
        payload.physician_id,
        booked_by,
        payload.department,
        payload.appointment_type,
        payload.scheduled_at,
        payload.duration_minutes.unwrap_or(30),
        payload.reason,
        payload.channel.unwrap_or_else(|| "in_person".to_string())
    )
    .fetch_one(&state.db)
    .await?;

let appointment = if appointment.channel.as_deref() == Some("telehealth") {
        let room_url = format!("https://meet.jit.si/hjarne-{}", appointment.id);

        sqlx::query_as!(
            AppointmentRow,
            r#"
            UPDATE appointments
            SET daily_room_url = $1
            WHERE id = $2
            RETURNING id, patient_id, physician_id, department,
                      appointment_type, status, scheduled_at,
                      duration_minutes, reason, channel, daily_room_url, created_at
            "#,
            room_url,
            appointment.id
        )
        .fetch_one(&state.db)
        .await?
    } else {
        appointment
    };

    if let Some(physician_id) = appointment.physician_id {
        if let Ok(Some(row)) = sqlx::query!(
            "SELECT id FROM auth_users WHERE entity_id = $1 AND entity_type = 'staff'",
            physician_id
        )
        .fetch_optional(&state.db)
        .await
        {
            crate::notifications::push_notification(
                &state,
                row.id,
                "appointment_scheduled",
                serde_json::json!({
                    "appointment_id": appointment.id,
                    "department": appointment.department,
                    "scheduled_at": appointment.scheduled_at,
                    "reason": appointment.reason,
                    "channel": appointment.channel,
                    "daily_room_url": appointment.daily_room_url,
                }),
            )
            .await;
        }
    }

    Ok(Json(ApiResponse::success(appointment)))
}

    

pub async fn update_appointment_status(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    Path(appointment_id): Path<Uuid>,
    Json(payload): Json<UpdateStatusRequest>,
) -> AppResult<Json<ApiResponse<AppointmentRow>>> {
    let valid_statuses = ["scheduled", "confirmed", "completed", "cancelled", "no_show"];
    if !valid_statuses.contains(&payload.status.as_str()) {
        return Err(AppError::BadRequest(format!(
            "invalid status: {}. must be one of: {}",
            payload.status,
            valid_statuses.join(", ")
        )));
    }

    let appointment = sqlx::query_as!(
        AppointmentRow,
        r#"
        UPDATE appointments
        SET status = $1,
            notes = COALESCE($2, notes),
            updated_at = NOW()
        WHERE id = $3
      RETURNING id, patient_id, physician_id, department,
                  appointment_type, status, scheduled_at,
                  duration_minutes, reason, channel, daily_room_url, created_at
        "#,
        payload.status,
        payload.notes,
        appointment_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("appointment {appointment_id} not found")))?;

    Ok(Json(ApiResponse::success(appointment)))
}
// ── Daily.co room creation ────────────────────────────────────────────────────

pub async fn create_daily_room(
    api_key: &str,
    room_name: &str,
    exp_minutes: i64,
) -> Option<String> {
    let exp = (chrono::Utc::now() + chrono::Duration::minutes(exp_minutes)).timestamp();

    let body = serde_json::json!({
        "name": room_name,
        "privacy": "private",
        "properties": {
            "exp": exp,
            "enable_chat": true,
            "enable_screenshare": false,
            "start_video_off": false,
            "start_audio_off": false,
        }
    });

    let client = reqwest::Client::new();
    let res = client
        .post("https://api.daily.co/v1/rooms")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .ok()?;

    let json: serde_json::Value = res.json().await.ok()?;
    json["url"].as_str().map(|s| s.to_string())
}
use axum::{extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::{errors::AppResult, response::ApiResponse};
use uuid::Uuid;

use crate::{middleware::AuthUser, state::AppState};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct NotificationRow {
    pub id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

pub async fn list_notifications(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<Vec<NotificationRow>>>> {
    let user_id = auth_user.0.sub;

    let notifications = sqlx::query_as_unchecked!(
        NotificationRow,
        r#"
        SELECT id, event_type, payload, status, created_at, read_at
        FROM notifications
        WHERE recipient_id = $1
          AND status IN ('pending', 'delivered', 'read')
        ORDER BY created_at DESC
        LIMIT 30
        "#,
        user_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(notifications)))
}

#[derive(Debug, Deserialize)]
pub struct MarkReadRequest {
    pub ids: Option<Vec<Uuid>>,
}

pub async fn mark_read(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(payload): Json<MarkReadRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let user_id = auth_user.0.sub;

    match payload.ids {
        // mark specific notifications read
        Some(ids) if !ids.is_empty() => {
            sqlx::query_unchecked!(
                r#"
                UPDATE notifications
                SET status = 'read', read_at = NOW()
                WHERE recipient_id = $1
                  AND id = ANY($2)
                  AND status IN ('pending', 'delivered')
                "#,
                user_id,
                &ids
            )
            .execute(&state.db)
            .await?;
        }
        // mark all read if no ids provided
        _ => {
            sqlx::query_unchecked!(
                r#"
                UPDATE notifications
                SET status = 'read', read_at = NOW()
                WHERE recipient_id = $1
                  AND status IN ('pending', 'delivered')
                "#,
                user_id
            )
            .execute(&state.db)
            .await?;
        }
    }

    Ok(Json(ApiResponse::success(serde_json::json!({
        "message": "notifications marked as read"
    }))))
}

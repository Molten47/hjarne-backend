use axum::{
    extract::{Path, State, Multipart},
    Extension, Json,
    response::IntoResponse,
    http::header,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use shared::{
    errors::{AppError, AppResult},
    response::ApiResponse,
};
use crate::{middleware::AuthUser, state::AppState};

// ── response shapes ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CommSender {
    pub id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub role: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AttachmentMeta {
    pub id: Uuid,
    pub file_name: String,
    pub file_type: String,
    pub file_size: i32,
    pub uploaded_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ClinicalCommRow {
    pub id: Uuid,
    pub case_file_id: Uuid,
    pub comm_type: String,
    pub subject: String,
    pub body: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub sender: CommSender,
    pub attachments: Vec<AttachmentMeta>,
}

// ── request shapes ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendCommPayload {
    pub recipient_id: Option<Uuid>,
    pub comm_type: String,
    pub subject: String,
    pub body: String,
}

// ── handlers ───────────────────────────────────────────────────────────────

pub async fn list_comms(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Extension(auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<Vec<ClinicalCommRow>>>> {
    let rows = sqlx::query_unchecked!(
        r#"
        SELECT
            cc.id, cc.case_file_id, cc.comm_type, cc.subject,
            cc.body, cc.status, cc.created_at,
            au.id AS sender_auth_id,
            s.first_name AS sender_first_name,
            s.last_name  AS sender_last_name,
            s.role       AS sender_role
        FROM clinical_communications cc
        JOIN auth_users au ON au.id = cc.sender_id
        JOIN staff s ON s.id = au.entity_id
        WHERE cc.case_file_id = $1
          AND (
              cc.recipient_id IS NULL
              OR cc.recipient_id = $2
              OR cc.sender_id   = $2
          )
        ORDER BY cc.created_at ASC
        "#,
        case_id,
        auth_user.0.sub,
    )
    .fetch_all(&state.db)
    .await?;

    let mut comms: Vec<ClinicalCommRow> = Vec::new();

    for row in rows {
        let attachments = sqlx::query_as_unchecked!(
            AttachmentMeta,
            r#"
            SELECT id, file_name, file_type, file_size, uploaded_at
            FROM clinical_attachments
            WHERE communication_id = $1
            ORDER BY uploaded_at ASC
            "#,
            row.id
        )
        .fetch_all(&state.db)
        .await?;

        comms.push(ClinicalCommRow {
            id: row.id,
            case_file_id: row.case_file_id,
            comm_type: row.comm_type,
            subject: row.subject,
            body: row.body,
            status: row.status,
            created_at: row.created_at,
            sender: CommSender {
                id: row.sender_auth_id,
                first_name: row.sender_first_name,
                last_name: row.sender_last_name,
                role: row.sender_role,
            },
            attachments,
        });
    }

    Ok(Json(ApiResponse::success(comms)))
}

pub async fn send_comm(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Extension(auth_user): Extension<AuthUser>,
    Json(payload): Json<SendCommPayload>,
) -> AppResult<Json<ApiResponse<ClinicalCommRow>>> {
    if !["note", "handoff", "upload"].contains(&payload.comm_type.as_str()) {
        return Err(AppError::BadRequest(
            "comm_type must be note, handoff, or upload".into(),
        ));
    }

    let comm = sqlx::query_unchecked!(
        r#"
        INSERT INTO clinical_communications
            (case_file_id, sender_id, recipient_id, comm_type, subject, body)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, case_file_id, comm_type, subject, body, status, created_at
        "#,
        case_id,
        auth_user.0.sub,
        payload.recipient_id,
        payload.comm_type,
        payload.subject,
        payload.body,
    )
    .fetch_one(&state.db)
    .await?;

    let sender = sqlx::query_unchecked!(
        r#"
        SELECT au.id, s.first_name, s.last_name, s.role
        FROM auth_users au
        JOIN staff s ON s.id = au.entity_id
        WHERE au.id = $1
        "#,
        auth_user.0.sub,
    )
    .fetch_one(&state.db)
    .await?;

    // notify recipient if targeted, not a broadcast
    if let Some(recipient_id) = payload.recipient_id {
        crate::notifications::push_notification(
            &state,
            recipient_id,
            "clinical_comm_received",
            serde_json::json!({
                "case_file_id": case_id,
                "comm_id": comm.id,
                "subject": comm.subject,
                "from": format!("{} {}", sender.first_name, sender.last_name),
                "type": comm.comm_type,
            }),
        )
        .await;
    }

    Ok(Json(ApiResponse::success(ClinicalCommRow {
        id: comm.id,
        case_file_id: comm.case_file_id,
        comm_type: comm.comm_type,
        subject: comm.subject,
        body: comm.body,
        status: comm.status,
        created_at: comm.created_at,
        sender: CommSender {
            id: sender.id,
            first_name: sender.first_name,
            last_name: sender.last_name,
            role: sender.role,
        },
        attachments: vec![],
    })))
}

pub async fn upload_attachment(
    State(state): State<AppState>,
    Path(comm_id): Path<Uuid>,
    mut multipart: Multipart,
) -> AppResult<Json<ApiResponse<AttachmentMeta>>> {
    let mut file_name = String::new();
    let mut file_type = String::new();
    let mut file_data: Vec<u8> = vec![];

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::BadRequest(format!("multipart error: {e}"))
    })? {
        if field.name().unwrap_or("") == "file" {
            file_name = field.file_name().unwrap_or("upload").to_string();
            file_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            file_data = field.bytes().await.map_err(|e| {
                AppError::BadRequest(format!("failed to read file: {e}"))
            })?.to_vec();
        }
    }

    if file_data.is_empty() {
        return Err(AppError::BadRequest("no file received".into()));
    }

    let file_size = file_data.len() as i32;

    let attachment = sqlx::query_as_unchecked!(
        AttachmentMeta,
        r#"
        INSERT INTO clinical_attachments
            (communication_id, file_name, file_type, file_size, file_data)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, file_name, file_type, file_size, uploaded_at
        "#,
        comm_id,
        file_name,
        file_type,
        file_size,
        file_data,
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(attachment)))
}

pub async fn serve_attachment(
    State(state): State<AppState>,
    Path(attachment_id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let row = sqlx::query_unchecked!(
        "SELECT file_name, file_type, file_data FROM clinical_attachments WHERE id = $1",
        attachment_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("attachment not found".into()))?;

    let headers = [
        (header::CONTENT_TYPE, row.file_type),
        (
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", row.file_name),
        ),
    ];

    Ok((headers, row.file_data))
}

pub async fn mark_comm_read(
    State(state): State<AppState>,
    Path(comm_id): Path<Uuid>,
    Extension(auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<()>>> {
    sqlx::query_unchecked!(
        r#"
        UPDATE clinical_communications
        SET status = 'read', read_at = NOW()
        WHERE id = $1 AND recipient_id = $2
        "#,
        comm_id,
        auth_user.0.sub,
    )
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(())))
}

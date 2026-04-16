use axum::{extract::{Path, Query, State}, Extension, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use shared::{
    errors::{AppError, AppResult},
    response::ApiResponse,
    types::Role,
};
use uuid::Uuid;
use sha2::{Sha256, Digest};

use crate::{middleware::AuthUser, state::AppState};
use auth::{create_access_token, hash_password, verify_password};

// ── helpers ──────────────────────────────────────────────────────────────────

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn generate_refresh_token() -> String {
    let id1 = Uuid::new_v4().as_simple().to_string();
    let id2 = Uuid::new_v4().as_simple().to_string();
    format!("{}{}", id1, id2)
}

async fn send_invite_email(
    api_key: &str,
    from: &str,
    to_email: &str,
    to_name: &str,
    mrn:&str,
    setup_url: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "from": from,
        "to": [to_email],
        "subject": "Your Hjärne HMS Patient Portal Invitation",
     "html": format!(r#"
    <div style="font-family:sans-serif;max-width:520px;margin:auto;padding:32px">
        <h2 style="color:#0284c7">Welcome to Hjärne HMS</h2>
        <p>Hello {to_name},</p>
        <p>You have been registered as a patient at Hjärne HMS.
           Click the button below to set up your patient portal account.</p>
        <a href="{setup_url}"
           style="display:inline-block;margin:24px 0;padding:12px 28px;
                  background:#0284c7;color:#fff;border-radius:8px;
                  text-decoration:none;font-weight:600">
            Set Up My Account
        </a>
        <div style="background:#f0f9ff;border-radius:8px;padding:16px 20px;margin:16px 0">
            <p style="margin:0 0 6px 0;font-weight:600;color:#0284c7">
                Your login details
            </p>
            <p style="margin:0;color:#334155;font-size:14px">
                <strong>Medical Record Number (MRN):</strong> {mrn}
            </p>
            <p style="margin:4px 0 0 0;color:#64748b;font-size:13px">
                Use this MRN and the password you set to log in at any time.
            </p>
        </div>
        <p style="color:#64748b;font-size:13px">
            This setup link expires in 72 hours. If you did not expect this
            email, please ignore it.
        </p>
    </div>
"#, to_name = to_name, mrn = mrn, setup_url = setup_url),
    });

    let res = client
        .post("https://api.resend.com/emails")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status().is_success() {
        Ok(())
    } else {
        let text = res.text().await.unwrap_or_default();
        Err(format!("resend error: {}", text))
    }
}

// ── request / response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendInviteRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct SetupAccountRequest {
    pub token:    String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct PortalLoginRequest {
    pub mrn:      String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct PortalLoginResponse {
    pub access_token:  String,
    pub refresh_token: String,
    pub expires_in:    i64,
    pub patient_id:    Uuid,
    pub mrn:           String,
    pub first_name:    String,
    pub last_name:     String,
}

#[derive(Debug, Serialize)]
pub struct PortalPatient {
    pub id:            Uuid,
    pub mrn:           String,
    pub first_name:    String,
    pub last_name:     String,
    pub date_of_birth: String,
    pub gender:        String,
    pub blood_group:   Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]

pub struct PortalAppointment {
    pub id:               Uuid,
    pub department:       String,
    pub appointment_type: String,
    pub status:           String,
    pub scheduled_at:     chrono::DateTime<Utc>,
    pub duration_minutes: i32,
    pub reason:           Option<String>,
    pub channel:          Option<String>,
    pub daily_room_url:   Option<String>,
    pub physician_name:   Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PortalCase {
    pub id:              Uuid,
    pub case_number:     String,
    pub department:      String,
    pub status:          String,
    pub chief_complaint: Option<String>,
    pub opened_at:       chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PortalDiagnosis {
    pub id:           Uuid,
    pub icd10_code:   String,
    pub description:  String,
    pub severity:     Option<String>,
    pub diagnosed_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitComplaintRequest {
    pub subject: String,
    pub body:    String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PortalComplaint {
    pub id:           Uuid,
    pub subject:      String,
    pub body:         String,
    pub status:       String,
    pub submitted_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PortalMessage {
    pub id:           Uuid,
    pub staff_id:     Uuid,
    pub staff_name:   String,
    pub body:         String,
    pub sender_type:  String,
    pub sent_at:      chrono::DateTime<Utc>,
    pub read_at:      Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub body:     String,
    pub staff_id: Uuid,
}

// ── POST /api/v1/patients/:id/invite ─────────────────────────────────────────
// Admin/desk sends portal invite email to patient

pub async fn send_invite(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(patient_id): Path<Uuid>,
    Json(payload): Json<SendInviteRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    let invited_by = auth_user.0.entity_id;

    // confirm patient exists
    let patient = sqlx::query!(
        "SELECT id, first_name, last_name, mrn FROM patients WHERE id = $1",
        patient_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("patient not found".to_string()))?;

    // invalidate any existing unconsumed invites
    sqlx::query!(
        r#"
        UPDATE portal_invites
        SET expires_at = NOW()
        WHERE patient_id = $1 AND consumed_at IS NULL
        "#,
        patient_id
    )
    .execute(&state.db)
    .await?;

    // create new invite
    let invite = sqlx::query!(
        r#"
        INSERT INTO portal_invites (patient_id, invited_by)
        VALUES ($1, $2)
        RETURNING token
        "#,
        patient_id,
        invited_by
    )
    .fetch_one(&state.db)
    .await?;

    let setup_url = format!(
        "{}/portal/setup?token={}",
        state.config.app_base_url,
        invite.token
    );

    let full_name = format!("{} {}", patient.first_name, patient.last_name);

    send_invite_email(
        &state.config.resend_api_key,
        &state.config.resend_from_email,
        &payload.email,
        &full_name,
        &patient.mrn,
        &setup_url,
    )
    .await
    .map_err(|e| {
        tracing::error!("email send failed: {}", e);
        AppError::Internal("failed to send invite email".to_string())
    })?;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "message": "invite sent",
        "expires_at": Utc::now() + chrono::Duration::hours(72),
    }))))
}

// ── POST /api/v1/portal/setup ─────────────────────────────────────────────────
// Patient sets their password using the invite token — no auth required

pub async fn setup_account(
    State(state): State<AppState>,
    Json(payload): Json<SetupAccountRequest>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    if payload.password.len() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".to_string()
        ));
    }

    let token_uuid = Uuid::parse_str(&payload.token)
        .map_err(|_| AppError::BadRequest("invalid token".to_string()))?;

    // fetch and validate invite
    let invite = sqlx::query!(
        r#"
        SELECT id, patient_id, consumed_at, expires_at
        FROM portal_invites
        WHERE token = $1
        "#,
        token_uuid
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::BadRequest("invalid or expired invite link".to_string()))?;

    if invite.consumed_at.is_some() {
        return Err(AppError::BadRequest("invite link already used".to_string()));
    }

    if invite.expires_at < Utc::now() {
        return Err(AppError::BadRequest("invite link has expired".to_string()));
    }

    // check no auth account exists yet
    let existing = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM auth_users WHERE entity_id = $1 AND entity_type = 'patient'",
        invite.patient_id
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0);

    if existing > 0 {
        return Err(AppError::Conflict(
            "portal account already exists for this patient".to_string()
        ));
    }

    // fetch patient MRN — used as their username/email field
    let patient = sqlx::query!(
        "SELECT mrn FROM patients WHERE id = $1",
        invite.patient_id
    )
    .fetch_one(&state.db)
    .await?;

    let password_hash = hash_password(&payload.password)?;

    // create auth_users row
    sqlx::query!(
        r#"
        INSERT INTO auth_users
            (entity_id, entity_type, email, password_hash, is_active, must_change_password)
        VALUES ($1, 'patient', $2, $3, TRUE, FALSE)
        "#,
        invite.patient_id,
        patient.mrn,         // MRN stored in email field — used as login identifier
        password_hash
    )
    .execute(&state.db)
    .await?;

    // consume the invite
 // consume the invite
    sqlx::query!(
        "UPDATE portal_invites SET consumed_at = NOW() WHERE id = $1",
        invite.id
    )
    .execute(&state.db)
    .await?;
    // mark patient portal as active
    sqlx::query!(
        "UPDATE patients SET portal_active = true WHERE id = $1",
        invite.patient_id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "message": "account created — you can now log in with your MRN and password"
    }))))
}

// ── POST /api/v1/portal/login ─────────────────────────────────────────────────

pub async fn portal_login(
    State(state): State<AppState>,
    Json(payload): Json<PortalLoginRequest>,
) -> AppResult<Json<ApiResponse<PortalLoginResponse>>> {
    // fetch auth record by MRN (stored in email field for patients)
    let user = sqlx::query!(
        r#"
        SELECT au.id, au.entity_id, au.password_hash, au.is_active
        FROM auth_users au
        WHERE au.email = $1 AND au.entity_type = 'patient'
        "#,
        payload.mrn
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Unauthorized("invalid MRN or password".to_string()))?;

    if !user.is_active {
        return Err(AppError::Unauthorized("account is deactivated".to_string()));
    }

    if !verify_password(&payload.password, &user.password_hash)? {
        return Err(AppError::Unauthorized("invalid MRN or password".to_string()));
    }

    let patient = sqlx::query!(
        "SELECT id, mrn, first_name, last_name FROM patients WHERE id = $1",
        user.entity_id
    )
    .fetch_one(&state.db)
    .await?;

    let access_token = create_access_token(
        user.id,
        user.entity_id,
        "patient".to_string(),
        vec![Role::Patient],
        None,
        &state.config.jwt_private_key,
        state.config.jwt_access_expiry_seconds,
    )?;

    let raw_refresh    = generate_refresh_token();
    let token_hash     = hash_token(&raw_refresh);
    let family_id      = Uuid::new_v4();
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

    sqlx::query!(
        "UPDATE auth_users SET last_login = NOW() WHERE id = $1",
        user.id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(PortalLoginResponse {
        access_token,
        refresh_token: raw_refresh,
        expires_in:    state.config.jwt_access_expiry_seconds,
        patient_id:    patient.id,
        mrn:           patient.mrn,
        first_name:    patient.first_name,
        last_name:     patient.last_name,
    })))
}

// ── GET /api/v1/portal/me ─────────────────────────────────────────────────────

pub async fn portal_me(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<PortalPatient>>> {
    let patient_id = auth_user.0.entity_id;

    let patient = sqlx::query!(
        r#"
        SELECT id, mrn, first_name, last_name,
               date_of_birth, gender, blood_group
        FROM patients WHERE id = $1
        "#,
        patient_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("patient not found".to_string()))?;

    Ok(Json(ApiResponse::success(PortalPatient {
        id:            patient.id,
        mrn:           patient.mrn,
        first_name:    patient.first_name,
        last_name:     patient.last_name,
        date_of_birth: patient.date_of_birth.to_string(),
        gender:        patient.gender,
        blood_group:   patient.blood_group,
    })))
}

// ── GET /api/v1/portal/appointments ──────────────────────────────────────────

pub async fn portal_appointments(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<Vec<PortalAppointment>>>> {
    let patient_id = auth_user.0.entity_id;
    
let appointments = sqlx::query_as!(
        PortalAppointment,
        r#"
        SELECT
            a.id, a.department, a.appointment_type, a.status,
            a.scheduled_at, a.duration_minutes, a.reason,
            a.channel, a.daily_room_url,
            CONCAT(s.first_name, ' ', s.last_name) AS "physician_name?"
        FROM appointments a
        LEFT JOIN staff s ON s.id = a.physician_id
        WHERE a.patient_id = $1
        ORDER BY a.scheduled_at DESC
        "#,
        patient_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(appointments)))
}

// ── GET /api/v1/portal/cases ──────────────────────────────────────────────────

pub async fn portal_cases(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<Vec<PortalCase>>>> {
    let patient_id = auth_user.0.entity_id;

    let cases = sqlx::query_as!(
        PortalCase,
        r#"
        SELECT id, case_number, department, status,
               chief_complaint, opened_at
        FROM case_files
        WHERE patient_id = $1
        ORDER BY opened_at DESC
        "#,
        patient_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(cases)))
}

// ── GET /api/v1/portal/cases/:id/diagnoses ───────────────────────────────────

pub async fn portal_diagnoses(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(case_id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<Vec<PortalDiagnosis>>>> {
    let patient_id = auth_user.0.entity_id;

    // ensure case belongs to this patient
    let owns = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!" FROM case_files WHERE id = $1 AND patient_id = $2"#,
        case_id, patient_id
    )
    .fetch_one(&state.db)
    .await?
    > 0;

    if !owns {
        return Err(AppError::Forbidden("not your case".to_string()));
    }

    let diagnoses = sqlx::query_as!(
        PortalDiagnosis,
        r#"
        SELECT id, icd10_code, description, severity, diagnosed_at
        FROM diagnoses
        WHERE case_file_id = $1
        ORDER BY diagnosed_at ASC
        "#,
        case_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(diagnoses)))
}

// ── POST /api/v1/portal/complaints ───────────────────────────────────────────

pub async fn submit_complaint(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(payload): Json<SubmitComplaintRequest>,
) -> AppResult<Json<ApiResponse<PortalComplaint>>> {
    let patient_id = auth_user.0.entity_id;

    if payload.subject.trim().is_empty() || payload.body.trim().is_empty() {
        return Err(AppError::BadRequest("subject and body are required".to_string()));
    }

    let complaint = sqlx::query_as!(
        PortalComplaint,
        r#"
        INSERT INTO portal_complaints (patient_id, subject, body)
        VALUES ($1, $2, $3)
        RETURNING id, subject, body, status, submitted_at
        "#,
        patient_id,
        payload.subject.trim(),
        payload.body.trim()
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(complaint)))
}

// ── GET /api/v1/portal/messages ───────────────────────────────────────────────

pub async fn portal_messages(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<Vec<PortalMessage>>>> {
    let patient_id = auth_user.0.entity_id;

    let messages = sqlx::query_as!(
        PortalMessage,
        r#"
        SELECT
            pm.id,
            pm.staff_id,
            CONCAT(s.first_name, ' ', s.last_name) AS "staff_name!",
            pm.body,
            pm.sender_type,
            pm.sent_at,
            pm.read_at
        FROM portal_messages pm
        JOIN staff s ON s.id = pm.staff_id
        WHERE pm.patient_id = $1
        ORDER BY pm.sent_at ASC  
        "#,
        patient_id
    )
    .fetch_all(&state.db)
    .await?;

    // mark unread messages from staff as read
    sqlx::query!(
        r#"
        UPDATE portal_messages
        SET read_at = NOW()
        WHERE patient_id = $1
          AND sender_type = 'staff'
          AND read_at IS NULL
        "#,
        patient_id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(messages)))
}

// ── POST /api/v1/portal/messages ──────────────────────────────────────────────

pub async fn send_portal_message(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(payload): Json<SendMessageRequest>,
) -> AppResult<Json<ApiResponse<PortalMessage>>> {
    let patient_id = auth_user.0.entity_id;

    if payload.body.trim().is_empty() {
        return Err(AppError::BadRequest("message body cannot be empty".to_string()));
    }

let message = sqlx::query_as!(
        PortalMessage,
        r#"
        WITH ins AS (
            INSERT INTO portal_messages (patient_id, staff_id, body, sender_type)
            VALUES ($1, $2, $3, 'patient')
            RETURNING id, staff_id, body, sender_type, sent_at, read_at
        )
        SELECT
            ins.id,
            ins.staff_id,
            CONCAT(s.first_name, ' ', s.last_name) AS "staff_name!",
            ins.body,
            ins.sender_type,
            ins.sent_at,
            ins.read_at
        FROM ins
        JOIN staff s ON s.id = ins.staff_id
        "#,
        patient_id,
        payload.staff_id,
        payload.body.trim()
    )
    .fetch_one(&state.db)
    .await?;

    // notify the physician via existing WS pipeline
    if let Ok(Some(row)) = sqlx::query!(
        "SELECT id FROM auth_users WHERE entity_id = $1 AND entity_type = 'staff'",
        payload.staff_id
    )
    .fetch_optional(&state.db)
    .await
    {
        crate::notifications::push_notification(
            &state,
            row.id,
            "portal_message",
            serde_json::json!({
                "patient_id": patient_id,
                "message_id": message.id,
                "preview":    &payload.body.chars().take(60).collect::<String>(),
            }),
        )
        .await;
    }

    Ok(Json(ApiResponse::success(message)))
    
}
// ── Profile structs ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PortalProfile {
    pub id:             Uuid,
    pub mrn:            String,
    pub first_name:     String,
    pub last_name:      String,
    pub date_of_birth:  chrono::NaiveDate,
    pub gender:         String,
    pub blood_group:    Option<String>,
    pub genotype:       Option<String>,
    pub height_cm:      Option<f64>,
    pub weight_kg:      Option<f64>,
    pub bmi:            Option<f64>,
    pub nationality:    Option<String>,
    pub phone:          Option<String>,
    pub email:          Option<String>,
    pub address_line1:  Option<String>,
    pub address_line2:  Option<String>,
    pub city:           Option<String>,
    pub state_province: Option<String>,
    pub zip_postal:     Option<String>,
    pub country:        Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfilePayload {
    pub nationality:    Option<String>,
    pub height_cm:      Option<f64>,
    pub weight_kg:      Option<f64>,
    pub phone:          Option<String>,
    pub email:          Option<String>,
    pub address_line1:  Option<String>,
    pub address_line2:  Option<String>,
    pub city:           Option<String>,
    pub state_province: Option<String>,
    pub zip_postal:     Option<String>,
    pub country:        Option<String>,
}

// ── GET /api/v1/portal/profile ────────────────────────────────────────────────

pub async fn get_portal_profile(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<PortalProfile>>> {
    let patient_id = auth_user.0.entity_id;

let profile = sqlx::query_as!(
        PortalProfile,
        r#"
        SELECT
            p.id, p.mrn, p.first_name, p.last_name, p.date_of_birth,
            p.gender, p.blood_group, p.genotype,
            p.height_cm, p.weight_kg, p.bmi, p.nationality,
            pc.phone         AS "phone?",
            pc.email         AS "email?",
            pc.address_line1 AS "address_line1?",
            pc.address_line2 AS "address_line2?",
            pc.city          AS "city?",
            pc.state_province AS "state_province?",
            pc.zip_postal    AS "zip_postal?",
            pc.country       AS "country?"
        FROM patients p
        LEFT JOIN patient_contacts pc ON pc.patient_id = p.id
        WHERE p.id = $1
        "#,
        patient_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("patient profile not found".into()))?;

    Ok(Json(ApiResponse::success(profile)))
}

// ── PATCH /api/v1/portal/profile ──────────────────────────────────────────────

pub async fn update_portal_profile(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(payload): Json<UpdateProfilePayload>,
) -> AppResult<Json<ApiResponse<PortalProfile>>> {
    let patient_id = auth_user.0.entity_id;

    let bmi = match (payload.height_cm, payload.weight_kg) {
        (Some(h), Some(w)) if h > 0.0 => {
            let hm = h / 100.0;
            Some((w / (hm * hm) * 10.0).round() / 10.0)
        }
        _ => None,
    };

    sqlx::query!(
        r#"
        UPDATE patients SET
            nationality = COALESCE($2, nationality),
            height_cm   = COALESCE($3, height_cm),
            weight_kg   = COALESCE($4, weight_kg),
            bmi         = CASE WHEN $3 IS NOT NULL AND $4 IS NOT NULL THEN $5 ELSE bmi END,
            updated_at  = NOW()
        WHERE id = $1
        "#,
        patient_id,
        payload.nationality,
        payload.height_cm,
        payload.weight_kg,
        bmi,
    )
    .execute(&state.db)
    .await?;

    sqlx::query!(
        r#"
        INSERT INTO patient_contacts
            (patient_id, phone, email, address_line1, address_line2,
             city, state_province, zip_postal, country)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, COALESCE($9, 'United States'))
        ON CONFLICT (patient_id) DO UPDATE SET
            phone          = COALESCE(EXCLUDED.phone,          patient_contacts.phone),
            email          = COALESCE(EXCLUDED.email,          patient_contacts.email),
            address_line1  = COALESCE(EXCLUDED.address_line1,  patient_contacts.address_line1),
            address_line2  = COALESCE(EXCLUDED.address_line2,  patient_contacts.address_line2),
            city           = COALESCE(EXCLUDED.city,           patient_contacts.city),
            state_province = COALESCE(EXCLUDED.state_province, patient_contacts.state_province),
            zip_postal     = COALESCE(EXCLUDED.zip_postal,     patient_contacts.zip_postal),
            country        = COALESCE(EXCLUDED.country,        patient_contacts.country)
        "#,
        patient_id,
        payload.phone,
        payload.email,
        payload.address_line1,
        payload.address_line2,
        payload.city,
        payload.state_province,
        payload.zip_postal,
        payload.country,
    )
    .execute(&state.db)
    .await?;

    get_portal_profile(State(state), Extension(auth_user)).await
}
// ── POST /api/v1/staff/patients/:id/messages ─────────────────────────────────
// Staff initiates or replies to a patient portal message thread

#[derive(Debug, Deserialize)]
pub struct StaffSendMessageRequest {
    pub body: String,
}

pub async fn staff_send_portal_message(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(patient_id): Path<Uuid>,
    Json(payload): Json<StaffSendMessageRequest>,
) -> AppResult<Json<ApiResponse<PortalMessage>>> {
    let staff_id = auth_user.0.entity_id;

    if payload.body.trim().is_empty() {
        return Err(AppError::BadRequest("message body cannot be empty".to_string()));
    }

    // confirm patient exists
    let exists = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!" FROM patients WHERE id = $1"#,
        patient_id
    )
    .fetch_one(&state.db)
    .await?
    > 0;

    if !exists {
        return Err(AppError::NotFound("patient not found".to_string()));
    }

    let message = sqlx::query_as!(
        PortalMessage,
        r#"
        WITH ins AS (
            INSERT INTO portal_messages (patient_id, staff_id, body, sender_type)
            VALUES ($1, $2, $3, 'staff')
            RETURNING id, staff_id, body, sender_type, sent_at, read_at
        )
        SELECT
            ins.id,
            ins.staff_id,
            CONCAT(s.first_name, ' ', s.last_name) AS "staff_name!",
            ins.body,
            ins.sender_type,
            ins.sent_at,
            ins.read_at
        FROM ins
        JOIN staff s ON s.id = ins.staff_id
        "#,
        patient_id,
        staff_id,
        payload.body.trim()
    )
    .fetch_one(&state.db)
    .await?;

    // notify patient via WS if they're connected
    if let Ok(Some(row)) = sqlx::query!(
        "SELECT id FROM auth_users WHERE entity_id = $1 AND entity_type = 'patient'",
        patient_id
    )
    .fetch_optional(&state.db)
    .await
    {
        crate::notifications::push_notification(
            &state,
            row.id,
            "portal_message",
            serde_json::json!({
                "staff_id":   staff_id,
                "message_id": message.id,
                "preview":    &payload.body.chars().take(60).collect::<String>(),
            }),
        )
        .await;
    }

    Ok(Json(ApiResponse::success(message)))
}
// ── GET /api/v1/patients/:id/messages ────────────────────────────────────────
// Staff views full message thread with a patient

pub async fn staff_get_patient_messages(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    Path(patient_id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<Vec<PortalMessage>>>> {
    let messages = sqlx::query_as!(
        PortalMessage,
        r#"
        SELECT
            pm.id,
            pm.staff_id,
            CONCAT(s.first_name, ' ', s.last_name) AS "staff_name!",
            pm.body,
            pm.sender_type,
            pm.sent_at,
            pm.read_at
        FROM portal_messages pm
        JOIN staff s ON s.id = pm.staff_id
        WHERE pm.patient_id = $1
        ORDER BY pm.sent_at ASC
        "#,
        patient_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(messages)))
}
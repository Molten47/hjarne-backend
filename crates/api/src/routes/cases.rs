use axum::{extract::{Path, State}, Extension, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::{
    errors::{AppError, AppResult},
    response::ApiResponse,
    types::Role
};
use uuid::Uuid;
use validator::Validate;

use crate::{middleware::AuthUser, state::AppState};

// ── shared role check ─────────────────────────────────────────────────────────
fn is_clinical(role: &Role) -> bool {
    matches!(role, Role::Physician | Role::Surgeon)
}

fn can_see_all(role: &Role) -> bool {
    matches!(role, Role::Admin | Role::Desk | Role::Nurse | Role::Pharmacist)
}

// ── types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CaseRow {
    pub id: Uuid,
    pub case_number: String,
    pub patient_id: Uuid,
    pub primary_physician_id: Option<Uuid>,
    pub department: String,
    pub status: String,
    pub admission_type: Option<String>,
    pub admitted_at: Option<DateTime<Utc>>,
    pub discharged_at: Option<DateTime<Utc>>,
    pub chief_complaint: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AssignmentRow {
    pub id: Uuid,
    pub staff_id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub role: String,
    pub assigned_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CaseListQuery {
    pub status: Option<String>,
    pub cursor_time: Option<DateTime<Utc>>,
    pub cursor_id: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct OpenCaseRequest {
    pub patient_id: Uuid,
    pub physician_id: Option<Uuid>,
    pub department: String,
    pub admission_type: Option<String>,
    pub chief_complaint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCaseStatusRequest {
    pub status: String,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct AddDiagnosisRequest {
    #[validate(length(min = 1, max = 10))]
    pub icd10_code: String,
    #[validate(length(min = 1))]
    pub description: String,
    pub severity: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DiagnosisRow {
    pub id: Uuid,
    pub case_file_id: Uuid,
    pub icd10_code: String,
    pub description: String,
    pub severity: Option<String>,
    pub diagnosed_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AssignStaffRequest {
    pub staff_id: Uuid,
}

// ── list_cases ────────────────────────────────────────────────────────────────

pub async fn list_cases(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    axum::extract::Query(params): axum::extract::Query<CaseListQuery>,
) -> AppResult<Json<ApiResponse<Vec<CaseRow>>>> {
    let role     = auth_user.0.roles.first().unwrap_or(&Role::Desk);
    let staff_id = auth_user.0.entity_id;
    let limit    = params.limit.unwrap_or(25).min(100);
    let status   = params.status.as_deref().map(|s| s.to_owned());

    let mut cases = if can_see_all(&role) {
        sqlx::query_as_unchecked!(
            CaseRow,
            r#"
            SELECT id, case_number, patient_id, primary_physician_id,
                   department, status, admission_type, admitted_at,
                   discharged_at, chief_complaint, opened_at, closed_at
            FROM case_files
            WHERE ($1::text IS NULL OR status = $1)
              AND (
                $2::timestamptz IS NULL OR $3::uuid IS NULL
                OR (opened_at, id) < ($2, $3)
              )
            ORDER BY opened_at DESC, id DESC
            LIMIT $4
            "#,
            status,
            params.cursor_time as Option<DateTime<Utc>>,
            params.cursor_id as Option<Uuid>,
            limit + 1
        )
        .fetch_all(&state.db)
        .await?
    } else if is_clinical(&role) {
        sqlx::query_as_unchecked!(
            CaseRow,
            r#"
            SELECT cf.id, cf.case_number, cf.patient_id, cf.primary_physician_id,
                   cf.department, cf.status, cf.admission_type, cf.admitted_at,
                   cf.discharged_at, cf.chief_complaint, cf.opened_at, cf.closed_at
            FROM case_files cf
            JOIN case_assignments ca ON ca.case_id = cf.id
            WHERE ca.staff_id = $1
              AND ($2::text IS NULL OR cf.status = $2)
              AND (
                $3::timestamptz IS NULL OR $4::uuid IS NULL
                OR (cf.opened_at, cf.id) < ($3, $4)
              )
            ORDER BY cf.opened_at DESC, cf.id DESC
            LIMIT $5
            "#,
            staff_id,
            status,
            params.cursor_time as Option<DateTime<Utc>>,
            params.cursor_id as Option<Uuid>,
            limit + 1
        )
        .fetch_all(&state.db)
        .await?
    } else {
        vec![]
    };

    let next_cursor = if cases.len() as i64 > limit {
        cases.pop();
        cases.last().map(|c| {
            format!("{},{}", c.opened_at.to_rfc3339(), c.id)
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

    Ok(Json(ApiResponse::success_with_meta(cases, meta)))
}

// ── get_case ──────────────────────────────────────────────────────────────────

pub async fn get_case(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(case_id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<CaseRow>>> {
    let role     = auth_user.0.roles.first().unwrap_or(&Role::Desk);
    let staff_id = auth_user.0.entity_id;

    let case_file = sqlx::query_as_unchecked!(
        CaseRow,
        r#"
        SELECT id, case_number, patient_id, primary_physician_id,
               department, status, admission_type, admitted_at,
               discharged_at, chief_complaint, opened_at, closed_at
        FROM case_files
        WHERE id = $1
        "#,
        case_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("case {case_id} not found")))?;

    // clinical staff can only access cases they're assigned to
    if is_clinical(&role) {
        let assigned = sqlx::query_scalar_unchecked!(
            "SELECT COUNT(*) FROM case_assignments WHERE case_id = $1 AND staff_id = $2",
            case_id,
            staff_id
        )
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0) > 0;

        if !assigned {
            return Err(AppError::Forbidden("not assigned to this case".to_string()));
        }
    }

    Ok(Json(ApiResponse::success(case_file)))
}

// ── open_case ─────────────────────────────────────────────────────────────────

pub async fn open_case(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(payload): Json<OpenCaseRequest>,
) -> AppResult<Json<ApiResponse<CaseRow>>> {
    let opened_by = auth_user.0.entity_id;

    let count: i64 = sqlx::query_scalar_unchecked!("SELECT COUNT(*) FROM case_files")
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0);

    let year        = chrono::Utc::now().format("%Y");
    let case_number = format!("CASE-{}-{:06}", year, count + 1);

    let admitted_at: Option<DateTime<Utc>> = match payload.admission_type.as_deref() {
        Some("inpatient") | Some("emergency") => Some(Utc::now()),
        _ => None,
    };

    let case_file = sqlx::query_as_unchecked!(
        CaseRow,
        r#"
        INSERT INTO case_files (
            case_number, patient_id, primary_physician_id,
            department, admission_type, chief_complaint,
            admitted_at, opened_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, case_number, patient_id, primary_physician_id,
                  department, status, admission_type, admitted_at,
                  discharged_at, chief_complaint, opened_at, closed_at
        "#,
        case_number,
        payload.patient_id,
        payload.physician_id,
        payload.department,
        payload.admission_type,
        payload.chief_complaint,
        admitted_at,
        opened_by
    )
    .fetch_one(&state.db)
    .await?;

    // auto-assign primary physician into case_assignments
    if let Some(physician_id) = case_file.primary_physician_id {
        sqlx::query_unchecked!(
            r#"
            INSERT INTO case_assignments (case_id, staff_id, role, assigned_by)
            VALUES ($1, $2, 'physician', $3)
            ON CONFLICT DO NOTHING
            "#,
            case_file.id,
            physician_id,
            opened_by
        )
        .execute(&state.db)
        .await?;

        // notify the assigned physician
        if let Ok(Some(row)) = sqlx::query_unchecked!(
            "SELECT id FROM auth_users WHERE entity_id = $1 AND entity_type = 'staff'",
            physician_id
        )
        .fetch_optional(&state.db)
        .await
        {
            crate::notifications::push_notification(
                &state,
                row.id,
                "case_opened",
                serde_json::json!({
                    "case_number": case_file.case_number,
                    "department":  case_file.department,
                    "case_id":     case_file.id,
                }),
            )
            .await;
        }
    }

    Ok(Json(ApiResponse::success(case_file)))
}

// ── assign_staff ──────────────────────────────────────────────────────────────

pub async fn assign_staff(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(case_id): Path<Uuid>,
    Json(payload): Json<AssignStaffRequest>,
) -> AppResult<Json<ApiResponse<AssignmentRow>>> {
    let requester_role = auth_user.0.roles.first().unwrap_or(&Role::Desk);
    let requester_id   = auth_user.0.entity_id;

    // only admin or a physician already on the case can assign others
    if !can_see_all(&requester_role) {
        let on_case = sqlx::query_scalar_unchecked!(
            "SELECT COUNT(*) FROM case_assignments WHERE case_id = $1 AND staff_id = $2",
            case_id,
            requester_id
        )
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0) > 0;

        if !on_case {
            return Err(AppError::Forbidden("must be assigned to this case to add team members".to_string()));
        }
    }

    // confirm the case exists
    let case_exists = sqlx::query_scalar_unchecked!(
        "SELECT COUNT(*) FROM case_files WHERE id = $1",
        case_id
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0) > 0;

    if !case_exists {
        return Err(AppError::NotFound(format!("case {case_id} not found")));
    }

    // fetch the staff member being assigned
    let staff = sqlx::query_unchecked!(
        "SELECT id, role, first_name, last_name FROM staff WHERE id = $1 AND is_active = TRUE",
        payload.staff_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("staff member not found".to_string()))?;

    // only physicians and surgeons can be assigned to cases
    if !matches!(staff.role.as_str(), "physician" | "surgeon") {
        return Err(AppError::BadRequest(
            "only physicians and surgeons can be assigned to cases".to_string()
        ));
    }

    // insert assignment
    let assignment = sqlx::query_as_unchecked!(
        AssignmentRow,
        r#"
        INSERT INTO case_assignments (case_id, staff_id, role, assigned_by)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (case_id, staff_id) DO NOTHING
        RETURNING id, staff_id,
                  $5::text AS "first_name!",
                  $6::text AS "last_name!",
                  $3::text AS "role!",
                  assigned_at
        "#,
        case_id,
        staff.id,
        staff.role,
        requester_id,
        staff.first_name,
        staff.last_name
    )
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::Conflict("staff member already assigned to this case".to_string()))?;

    // notify the newly assigned staff
    if let Ok(Some(row)) = sqlx::query_unchecked!(
        "SELECT id FROM auth_users WHERE entity_id = $1 AND entity_type = 'staff'",
        staff.id
    )
    .fetch_optional(&state.db)
    .await
    {
        crate::notifications::push_notification(
            &state,
            row.id,
            "case_assigned",
            serde_json::json!({ "case_id": case_id }),
        )
        .await;
    }

    Ok(Json(ApiResponse::success(assignment)))
}

// ── list_assignments ──────────────────────────────────────────────────────────

pub async fn list_assignments(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(case_id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<Vec<AssignmentRow>>>> {
    let role     = auth_user.0.roles.first().unwrap_or(&Role::Desk);
    let staff_id = auth_user.0.entity_id;

    // clinical staff must be on the case to see its team
    if is_clinical(&role) {
        let on_case = sqlx::query_scalar_unchecked!(
            "SELECT COUNT(*) FROM case_assignments WHERE case_id = $1 AND staff_id = $2",
            case_id,
            staff_id
        )
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0) > 0;

        if !on_case {
            return Err(AppError::Forbidden("not assigned to this case".to_string()));
        }
    }

    let assignments = sqlx::query_as_unchecked!(
        AssignmentRow,
        r#"
        SELECT ca.id, ca.staff_id,
               s.first_name AS "first_name!",
               s.last_name  AS "last_name!",
               ca.role      AS "role!",
               ca.assigned_at
        FROM case_assignments ca
        JOIN staff s ON s.id = ca.staff_id
        WHERE ca.case_id = $1
        ORDER BY ca.assigned_at ASC
        "#,
        case_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(assignments)))
}

// ── update_case_status ────────────────────────────────────────────────────────

pub async fn update_case_status(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(case_id): Path<Uuid>,
    Json(payload): Json<UpdateCaseStatusRequest>,
) -> AppResult<Json<ApiResponse<CaseRow>>> {
    let valid = ["open", "closed", "discharged"];
    if !valid.contains(&payload.status.as_str()) {
        return Err(AppError::BadRequest(format!("invalid status: {}", payload.status)));
    }

    let closed_by    = auth_user.0.entity_id;
    let closed_at    = match payload.status.as_str() {
        "closed" | "discharged" => Some(Utc::now()),
        _ => None,
    };
    let discharged_at = match payload.status.as_str() {
        "discharged" => Some(Utc::now()),
        _ => None,
    };
    let closed_by_id = match payload.status.as_str() {
        "closed" | "discharged" => Some(closed_by),
        _ => None,
    };

    let case_file = sqlx::query_as_unchecked!(
        CaseRow,
        r#"
        UPDATE case_files
        SET status        = $1,
            notes         = COALESCE($2, notes),
            closed_at     = $3,
            closed_by     = $4,
            discharged_at = COALESCE($5, discharged_at),
            updated_at    = NOW()
        WHERE id = $6
        RETURNING id, case_number, patient_id, primary_physician_id,
                  department, status, admission_type, admitted_at,
                  discharged_at, chief_complaint, opened_at, closed_at
        "#,
        payload.status,
        payload.notes,
        closed_at,
        closed_by_id,
        discharged_at,
        case_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("case {case_id} not found")))?;

    Ok(Json(ApiResponse::success(case_file)))
}

// ── add_diagnosis ─────────────────────────────────────────────────────────────

pub async fn add_diagnosis(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(case_id): Path<Uuid>,
    Json(payload): Json<AddDiagnosisRequest>,
) -> AppResult<Json<ApiResponse<DiagnosisRow>>> {
    payload.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let physician_id = auth_user.0.entity_id;

    let diagnosis = sqlx::query_as_unchecked!(
        DiagnosisRow,
        r#"
        INSERT INTO diagnoses (case_file_id, physician_id, icd10_code, description, severity, notes)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, case_file_id, icd10_code, description, severity, diagnosed_at
        "#,
        case_id,
        physician_id,
        payload.icd10_code,
        payload.description,
        payload.severity,
        payload.notes
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(diagnosis)))
}

use axum::{
    extract::{Path, Query, State},
    Extension,
    Json,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use shared::{
    errors::{AppError, AppResult},
    response::ApiResponse,
};
use uuid::Uuid;
use validator::Validate;

use crate::{middleware::AuthUser, state::AppState};

#[derive(Debug, Deserialize)]
pub struct PatientSearchQuery {
    pub q: Option<String>,
    pub mrn: Option<String>,
    pub limit: Option<i64>,
    pub after: Option<Uuid>,
    pub cursor: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PatientSummary {
    pub id: Uuid,
    pub mrn: String,
    pub first_name: String,
    pub last_name: String,
    pub date_of_birth: NaiveDate,
    pub gender: String,
    pub blood_group: Option<String>,
    pub portal_active: bool,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreatePatientRequest {
    #[validate(length(min = 1, max = 100))]
    pub first_name: String,
    #[validate(length(min = 1, max = 100))]
    pub last_name: String,
    pub date_of_birth: NaiveDate,
    #[validate(length(min = 1, max = 20))]
    pub gender: String,
    pub blood_group: Option<String>,
    pub genotype: Option<String>,
    pub height_cm: Option<f64>,
    pub weight_kg: Option<f64>,
    pub nationality: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PatientFull {
    pub id:            Uuid,
    pub mrn:           String,
    pub first_name:    String,
    pub last_name:     String,
    pub date_of_birth: NaiveDate,
    pub gender:        String,
    pub blood_group:   Option<String>,
    pub portal_active: bool,
    pub phone:         Option<String>,
    pub email:         Option<String>,
    pub address:       Option<String>,
}

pub async fn list_patients(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<PatientSearchQuery>,
) -> AppResult<Json<ApiResponse<Vec<PatientSummary>>>> {
    use shared::types::Role;

    let limit    = params.limit.unwrap_or(25).min(100);
    let cursor   = params.cursor.or(params.after);
    let staff_id = auth_user.0.entity_id;
    let is_admin = auth_user.0.roles.contains(&Role::Admin);

    let mut cache     = state.cache.clone();
    let cursor_str    = cursor.map(|u| u.to_string()).unwrap_or_default();

    // ── Search branch ────────────────────────────────────────────────────────
    if let Some(q) = &params.q {
        let pattern = format!("%{}%", q.to_lowercase());

        let cache_key = if is_admin {
            cache::keys::patient_search(q)
        } else {
            cache::keys::patient_search_staff(staff_id, q)
        };

        if let Ok(Some(cached)) = cache.get::<Vec<PatientSummary>>(&cache_key).await {
            return Ok(Json(ApiResponse::success(cached)));
        }

        let patients = if is_admin {
            sqlx::query_as!(
                PatientSummary,
                r#"
                SELECT id, mrn, first_name, last_name, date_of_birth,
                       gender, blood_group, portal_active
                FROM patients
                WHERE LOWER(last_name)  LIKE $1
                   OR LOWER(first_name) LIKE $1
                   OR mrn ILIKE $1
                ORDER BY last_name, first_name
                LIMIT $2
                "#,
                pattern, limit
            )
            .fetch_all(&state.db)
            .await?
        } else {
            sqlx::query_as!(
                PatientSummary,
                r#"
                SELECT DISTINCT p.id, p.mrn, p.first_name, p.last_name,
                       p.date_of_birth, p.gender, p.blood_group, p.portal_active
                FROM patients p
                JOIN case_files cf      ON cf.patient_id = p.id
                JOIN case_assignments ca ON ca.case_id   = cf.id
                WHERE ca.staff_id = $1
                  AND (LOWER(p.last_name)  LIKE $2
                       OR LOWER(p.first_name) LIKE $2
                       OR p.mrn ILIKE $2)
                ORDER BY p.last_name, p.first_name
                LIMIT $3
                "#,
                staff_id, pattern, limit
            )
            .fetch_all(&state.db)
            .await?
        };

        let _ = cache.set(&cache_key, &patients, 120).await;
        return Ok(Json(ApiResponse::success(patients)));
    }

    // ── List branch ──────────────────────────────────────────────────────────
    let cache_key = if is_admin {
        cache::keys::patient_list_admin(&cursor_str, limit)
    } else {
        cache::keys::patient_list_staff(staff_id, &cursor_str, limit)
    };

    if let Ok(Some(cached)) = cache.get::<Vec<PatientSummary>>(&cache_key).await {
        return Ok(Json(ApiResponse::success(cached)));
    }

    let mut patients = if is_admin {
        sqlx::query_as!(
            PatientSummary,
            r#"
            SELECT id, mrn, first_name, last_name, date_of_birth,
                   gender, blood_group, portal_active
            FROM patients
            WHERE ($1::uuid IS NULL OR id > $1)
            ORDER BY id
            LIMIT $2
            "#,
            cursor as Option<Uuid>, limit + 1
        )
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as!(
            PatientSummary,
            r#"
            SELECT DISTINCT p.id, p.mrn, p.first_name, p.last_name,
                   p.date_of_birth, p.gender, p.blood_group, p.portal_active
            FROM patients p
            JOIN case_files cf       ON cf.patient_id = p.id
            JOIN case_assignments ca ON ca.case_id    = cf.id
            WHERE ca.staff_id = $1
              AND ($2::uuid IS NULL OR p.id > $2)
            ORDER BY p.id
            LIMIT $3
            "#,
            staff_id, cursor as Option<Uuid>, limit + 1
        )
        .fetch_all(&state.db)
        .await?
    };

    let next_cursor = if patients.len() as i64 > limit {
        patients.pop();
        patients.last().map(|p| p.id.to_string())
    } else {
        None
    };

    let _ = cache.set(&cache_key, &patients, 120).await;

    let meta = shared::response::Meta {
        page:       None,
        total:      None,
        next_cursor,
        request_id: None,
    };

    Ok(Json(ApiResponse::success_with_meta(patients, meta)))
}



pub async fn get_patient(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    Path(patient_id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<PatientFull>>> {
    let patient = sqlx::query_as!(
        PatientFull,
        r#"
        SELECT
            p.id, p.mrn, p.first_name, p.last_name, p.date_of_birth,
            p.gender, p.blood_group, p.portal_active,
            pc.phone        AS "phone?",
            pc.email        AS "email?",
            pc.address_line1 AS "address?"
        FROM patients p
        LEFT JOIN patient_contacts pc ON pc.patient_id = p.id
        WHERE p.id = $1
        "#,
        patient_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("patient {patient_id} not found")))?;

    Ok(Json(ApiResponse::success(patient)))
}

pub async fn create_patient(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    Json(payload): Json<CreatePatientRequest>,
) -> AppResult<Json<ApiResponse<PatientSummary>>> {
    payload
        .validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    // generate MRN -- format HJN-2026-XXXXXX
    let year = chrono::Utc::now().format("%Y");
    let seq: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM patients"
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0);
    let mrn = format!("HJN-{}-{:06}", year, seq + 1);

    // calculate BMI if height and weight provided
    let bmi = match (payload.height_cm, payload.weight_kg) {
        (Some(h), Some(w)) if h > 0.0 => {
            let height_m = h / 100.0;
            Some((w / (height_m * height_m) * 10.0).round() / 10.0)
        }
        _ => None,
    };

let patient = sqlx::query_as!(
    PatientSummary,
    r#"
    INSERT INTO patients (
        mrn, first_name, last_name, date_of_birth,
        gender, blood_group, genotype,
        height_cm, weight_kg, bmi, nationality
    )
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
    RETURNING id, mrn, first_name, last_name, date_of_birth, gender, blood_group, portal_active
    "#,
    mrn,
    payload.first_name,
    payload.last_name,
    payload.date_of_birth,
    payload.gender,
    payload.blood_group,
    payload.genotype,
    payload.height_cm,
    payload.weight_kg,
    bmi,
    payload.nationality
)
.fetch_one(&state.db)
.await?;
// invalidate admin list cache so new patient appears immediately
    let mut cache = state.cache.clone();
    let _ = cache.invalidate_pattern("patients:list:admin:*").await;

    Ok(Json(ApiResponse::success(patient)))

}
use axum::{extract::{Path, State}, Extension, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::{
    errors::{AppError, AppResult},
    response::ApiResponse,
};
use uuid::Uuid;

use crate::{middleware::AuthUser, state::AppState};

#[derive(Debug, Serialize)]
pub struct PrescriptionItem {
    pub id: Uuid,
    pub drug_id: Uuid,
    pub drug_name: String,
    pub generic_name: Option<String>,
    pub dosage: String,
    pub frequency: String,
    pub route: String,
    pub duration_days: Option<i32>,
    pub instructions: Option<String>,
    pub contraindication_flagged: bool,
    pub is_controlled: bool,
}

#[derive(Debug, Serialize)]
pub struct PrescriptionRow {
    pub id: Uuid,
    pub case_file_id: Uuid,
    pub prescribed_by: Uuid,
    pub status: String,
    pub prescribed_at: DateTime<Utc>,
    pub physician_approved: bool,
    pub ai_confidence: Option<f64>,
    pub ai_recommendation: Option<String>,
    pub items: Vec<PrescriptionItem>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePrescriptionRequest {
    pub items: Vec<PrescriptionItemRequest>,
    pub ai_recommendation: Option<String>,
    pub ai_confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct PrescriptionItemRequest {
    pub drug_id: Uuid,
    pub dosage: String,
    pub frequency: String,
    pub route: String,
    pub duration_days: Option<i32>,
    pub instructions: Option<String>,
}

// shared helper — fetch items for a prescription
async fn fetch_items(
    db: &sqlx::PgPool,
    prescription_id: Uuid,
) -> Result<Vec<PrescriptionItem>, sqlx::Error> {
    let rows = sqlx::query_unchecked!(
        r#"
        SELECT
            pi.id, pi.drug_id, d.name AS drug_name,
            d.generic_name, pi.dosage, pi.frequency,
            pi.route, pi.duration_days, pi.instructions,
            pi.contraindication_flagged, d.is_controlled
        FROM prescription_items pi
        JOIN drugs d ON d.id = pi.drug_id
        WHERE pi.prescription_id = $1
        ORDER BY pi.created_at ASC
        "#,
        prescription_id
    )
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(|r| PrescriptionItem {
        id: r.id,
        drug_id: r.drug_id,
        drug_name: r.drug_name,
        generic_name: r.generic_name,
        dosage: r.dosage,
        frequency: r.frequency,
        route: r.route,
        duration_days: r.duration_days,
        instructions: r.instructions,
        contraindication_flagged: r.contraindication_flagged,
        is_controlled: r.is_controlled,
    }).collect())
}

pub async fn create_prescription(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(case_id): Path<Uuid>,
    Json(payload): Json<CreatePrescriptionRequest>,
) -> AppResult<Json<ApiResponse<PrescriptionRow>>> {
    let physician_id = auth_user.0.entity_id;

    let case_exists = sqlx::query_scalar_unchecked!(
        "SELECT COUNT(*) FROM case_files WHERE id = $1 AND status = 'open'",
        case_id
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0) > 0;

    if !case_exists {
        return Err(AppError::NotFound(format!("open case {case_id} not found")));
    }

    let prescription = sqlx::query_unchecked!(
        r#"
        INSERT INTO prescriptions (
            case_file_id, prescribed_by, ai_recommendation,
            ai_confidence, physician_approved, approved_at, status
        )
        VALUES ($1, $2, $3, $4, TRUE, NOW(), 'approved')
        RETURNING id, case_file_id, prescribed_by, status,
                  prescribed_at, physician_approved, ai_confidence, ai_recommendation
        "#,
        case_id,
        physician_id,
        payload.ai_recommendation,
        payload.ai_confidence
    )
    .fetch_one(&state.db)
    .await?;

    for item in &payload.items {
        let flagged = sqlx::query_scalar_unchecked!(
            r#"
            SELECT COUNT(*) FROM prescription_items pi
            JOIN drugs d ON d.id = pi.drug_id
            JOIN prescriptions p ON p.id = pi.prescription_id
            JOIN case_files cf ON cf.id = p.case_file_id
            WHERE cf.patient_id = (SELECT patient_id FROM case_files WHERE id = $1)
              AND p.status != 'cancelled'
              AND d.interactions && (SELECT interactions FROM drugs WHERE id = $2)
            "#,
            case_id,
            item.drug_id
        )
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0) > 0;

        sqlx::query_unchecked!(
            r#"
            INSERT INTO prescription_items (
                prescription_id, drug_id, dosage, frequency,
                route, duration_days, instructions, contraindication_flagged
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            prescription.id,
            item.drug_id,
            item.dosage,
            item.frequency,
            item.route,
            item.duration_days,
            item.instructions,
            flagged
        )
        .execute(&state.db)
        .await?;
    }

    let items = fetch_items(&state.db, prescription.id).await?;

    Ok(Json(ApiResponse::success(PrescriptionRow {
        id: prescription.id,
        case_file_id: prescription.case_file_id,
        prescribed_by: prescription.prescribed_by,
        status: prescription.status,
        prescribed_at: prescription.prescribed_at,
        physician_approved: prescription.physician_approved,
        ai_confidence: prescription.ai_confidence,
        ai_recommendation: prescription.ai_recommendation,
        items,
    })))
}

pub async fn get_pharmacy_queue(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<Vec<PrescriptionRow>>>> {
    let prescriptions = sqlx::query_unchecked!(
        r#"
        SELECT id, case_file_id, prescribed_by, status,
               prescribed_at, physician_approved, ai_confidence, ai_recommendation
        FROM prescriptions
        WHERE status = 'approved'
          AND physician_approved = TRUE
        ORDER BY prescribed_at ASC
        LIMIT 50
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let mut queue: Vec<PrescriptionRow> = Vec::new();

    for p in prescriptions {
        let items = fetch_items(&state.db, p.id).await?;
        queue.push(PrescriptionRow {
            id: p.id,
            case_file_id: p.case_file_id,
            prescribed_by: p.prescribed_by,
            status: p.status,
            prescribed_at: p.prescribed_at,
            physician_approved: p.physician_approved,
            ai_confidence: p.ai_confidence,
            ai_recommendation: p.ai_recommendation,
            items,
        });
    }

    Ok(Json(ApiResponse::success(queue)))
}

pub async fn dispense_prescription(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Path(prescription_id): Path<Uuid>,
) -> AppResult<Json<ApiResponse<PrescriptionRow>>> {
    let pharmacist_id = auth_user.0.entity_id;

    let prescription = sqlx::query_unchecked!(
        r#"
        UPDATE prescriptions
        SET status = 'dispensed'
        WHERE id = $1 AND status = 'approved'
        RETURNING id, case_file_id, prescribed_by, status,
                  prescribed_at, physician_approved, ai_confidence, ai_recommendation
        "#,
        prescription_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(
        format!("approved prescription {prescription_id} not found")
    ))?;

    let items = sqlx::query_unchecked!(
        "SELECT id, drug_id FROM prescription_items WHERE prescription_id = $1",
        prescription_id
    )
    .fetch_all(&state.db)
    .await?;

    for item in &items {
        sqlx::query_unchecked!(
            r#"
            INSERT INTO dispensary_log (drug_id, prescription_item_id, dispensed_by, quantity)
            VALUES ($1, $2, $3, 1)
            "#,
            item.drug_id,
            item.id,
            pharmacist_id
        )
        .execute(&state.db)
        .await?;

        sqlx::query_unchecked!(
            r#"
            UPDATE drug_stock
            SET quantity_on_hand = GREATEST(quantity_on_hand - 1, 0),
                updated_at = NOW()
            WHERE drug_id = $1
            "#,
            item.drug_id
        )
        .execute(&state.db)
        .await?;

        // Check if stock just hit or crossed reorder threshold
        if let Ok(Some(stock)) = sqlx::query_unchecked!(
            r#"
            SELECT ds.quantity_on_hand, ds.reorder_threshold, d.name AS drug_name
            FROM drug_stock ds
            JOIN drugs d ON d.id = ds.drug_id
            WHERE ds.drug_id = $1
              AND ds.quantity_on_hand <= ds.reorder_threshold
            "#,
            item.drug_id
        )
        .fetch_optional(&state.db)
        .await
        {
            // Notify all pharmacists and admins
            let recipients = sqlx::query_unchecked!(
                r#"
                SELECT au.id
                FROM auth_users au
                JOIN staff s ON s.id = au.entity_id
                WHERE au.entity_type = 'staff'
                  AND s.role IN ('pharmacist', 'admin')
                "#
            )
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

            for r in recipients {
                crate::notifications::push_notification(
                    &state,
                    r.id,
                    "low_stock_alert",
                    serde_json::json!({
                        "drug_id":           item.drug_id,
                        "drug_name":         stock.drug_name,
                        "quantity_on_hand":  stock.quantity_on_hand,
                        "reorder_threshold": stock.reorder_threshold,
                        "message": format!(
                            "{} is low — {} unit(s) remaining (reorder at {})",
                            stock.drug_name,
                            stock.quantity_on_hand,
                            stock.reorder_threshold
                        ),
                    }),
                )
                .await;
            }
        }
    }

    if let Ok(Some(row)) = sqlx::query_unchecked!(
        "SELECT id FROM auth_users WHERE entity_id = $1 AND entity_type = 'staff'",
        prescription.prescribed_by
    )
    .fetch_optional(&state.db)
    .await
    {
        crate::notifications::push_notification(
            &state,
            row.id,
            "prescription_dispensed",
            serde_json::json!({
                "prescription_id": prescription.id,
                "case_id": prescription.case_file_id,
            }),
        )
        .await;
    }

    let full_items = fetch_items(&state.db, prescription.id).await?;

    Ok(Json(ApiResponse::success(PrescriptionRow {
        id: prescription.id,
        case_file_id: prescription.case_file_id,
        prescribed_by: prescription.prescribed_by,
        status: prescription.status,
        prescribed_at: prescription.prescribed_at,
        physician_approved: prescription.physician_approved,
        ai_confidence: prescription.ai_confidence,
        ai_recommendation: prescription.ai_recommendation,
        items: full_items,
    })))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DrugStockRow {
    pub drug_id: Uuid,
    pub name: String,
    pub category: Option<String>,
    pub unit: Option<String>,
    pub quantity_on_hand: i32,
    pub reorder_threshold: i32,
    pub is_low: bool,
}

pub async fn get_drug_stock(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
) -> AppResult<Json<ApiResponse<Vec<DrugStockRow>>>> {
    let stock = sqlx::query_as_unchecked!(
        DrugStockRow,
        r#"
        SELECT
            ds.drug_id,
            d.name,
            d.category,
            ds.unit,
            ds.quantity_on_hand,
            ds.reorder_threshold,
            (ds.quantity_on_hand <= ds.reorder_threshold) AS "is_low!"
        FROM drug_stock ds
        JOIN drugs d ON d.id = ds.drug_id
        WHERE d.is_active = TRUE
        ORDER BY d.name ASC
        "#
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(stock)))
}

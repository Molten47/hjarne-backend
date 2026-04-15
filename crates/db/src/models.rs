use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Patient {
    pub id: Uuid,
    pub mrn: String,
    pub first_name: String,
    pub last_name: String,
    pub date_of_birth: chrono::NaiveDate,
    pub gender: String,
    pub blood_group: Option<String>,
    pub genotype: Option<String>,
    pub height_cm: Option<f64>,
    pub weight_kg: Option<f64>,
    pub bmi: Option<f64>,
    pub nationality: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Staff {
    pub id: Uuid,
    pub staff_code: String,
    pub first_name: String,
    pub last_name: String,
    pub role: String,
    pub department: Option<String>,
    pub specialization: Option<String>,
    pub license_number: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuthUser {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub entity_type: String,
    pub email: String,
    pub password_hash: String,
    pub is_active: bool,
    pub last_login: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CaseFile {
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
    pub notes: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Appointment {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub physician_id: Option<Uuid>,
    pub booked_by: Option<Uuid>,
    pub department: String,
    pub appointment_type: String,
    pub status: String,
    pub scheduled_at: DateTime<Utc>,
    pub duration_minutes: Option<i32>,
    pub reason: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Prescription {
    pub id: Uuid,
    pub case_file_id: Uuid,
    pub prescribed_by: Uuid,
    pub status: String,
    pub prescribed_at: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub ai_recommendation: Option<String>,
    pub ai_confidence: Option<String>,
    pub physician_approved: bool,
    pub approved_at: Option<DateTime<Utc>>,
}
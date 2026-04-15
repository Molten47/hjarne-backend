use axum::{extract::State, Json};
use serde_json::{json, Value};
use crate::state::AppState;
use shared::errors::AppResult;

pub async fn health_check(State(state): State<AppState>) -> AppResult<Json<Value>> {
    // ping the database
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|e| shared::errors::AppError::Internal(format!("db ping failed: {e}")))?;

    Ok(Json(json!({
        "status": "ok",
        "service": "hjarne-api",
        "version": env!("CARGO_PKG_VERSION")
    })))
}
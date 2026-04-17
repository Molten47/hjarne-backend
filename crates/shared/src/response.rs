use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: Option<T>,
    pub meta: Option<Meta>,
    pub error: Option<ErrorBody>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub page: Option<u32>,
    pub total: Option<u64>,
    pub next_cursor: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub field: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            data: Some(data),
            meta: None,
            error: None,
        }
    }

    pub fn success_with_meta(data: T, meta: Meta) -> Self {
        Self {
            data: Some(data),
            meta: Some(meta),
            error: None,
        }
    }
}

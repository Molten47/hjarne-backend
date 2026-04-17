use redis::{aio::ConnectionManager, Client};
use shared::errors::{AppError, AppResult};
use tracing::info;

#[derive(Clone)]
pub struct CacheClient {
    pub manager: ConnectionManager,
}

impl CacheClient {
    pub async fn new(redis_url: &str) -> AppResult<Self> {
        info!("connecting to redis...");

        let client = Client::open(redis_url)
            .map_err(|e| AppError::Internal(format!("redis client error: {e}")))?;

        let manager = ConnectionManager::new(client)
            .await
            .map_err(|e| AppError::Internal(format!("redis connection manager error: {e}")))?;

        info!("redis connection established");
        Ok(Self { manager })
    }
}

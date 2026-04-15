use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};
use shared::errors::{AppError, AppResult};
use tracing::instrument;

use crate::client::CacheClient;

impl CacheClient {
    #[instrument(skip(self))]
    pub async fn get<T: DeserializeOwned>(&mut self, key: &str) -> AppResult<Option<T>> {
        let value: Option<String> = self
            .manager
            .get(key)
            .await
            .map_err(|e| AppError::Internal(format!("redis get error: {e}")))?;

        match value {
            None => Ok(None),
            Some(v) => {
                let parsed = serde_json::from_str(&v)
                    .map_err(|e| AppError::Internal(format!("cache deserialize error: {e}")))?;
                Ok(Some(parsed))
            }
        }
    }

    #[instrument(skip(self, value))]
    pub async fn set<T: Serialize>(
        &mut self,
        key: &str,
        value: &T,
        ttl_seconds: u64,
    ) -> AppResult<()> {
        let serialized = serde_json::to_string(value)
            .map_err(|e| AppError::Internal(format!("cache serialize error: {e}")))?;

        self.manager
            .set_ex::<_, _, ()>(key, serialized, ttl_seconds)
            .await
            .map_err(|e| AppError::Internal(format!("redis set error: {e}")))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn invalidate(&mut self, key: &str) -> AppResult<()> {
        self.manager
            .del::<_, ()>(key)
            .await
            .map_err(|e| AppError::Internal(format!("redis del error: {e}")))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn invalidate_pattern(&mut self, pattern: &str) -> AppResult<()> {
        let keys: Vec<String> = self
            .manager
            .keys(pattern)
            .await
            .map_err(|e| AppError::Internal(format!("redis keys error: {e}")))?;

        if keys.is_empty() {
            return Ok(());
        }

        self.manager
            .del::<_, ()>(keys)
            .await
            .map_err(|e| AppError::Internal(format!("redis del pattern error: {e}")))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn exists(&mut self, key: &str) -> AppResult<bool> {
        let result: bool = self
            .manager
            .exists(key)
            .await
            .map_err(|e| AppError::Internal(format!("redis exists error: {e}")))?;

        Ok(result)
    }

    #[instrument(skip(self, token_hash))]
    pub async fn store_refresh_token(
        &mut self,
        key: &str,
        token_hash: &str,
        ttl_seconds: u64,
    ) -> AppResult<()> {
        self.manager
            .set_ex::<_, _, ()>(key, token_hash, ttl_seconds)
            .await
            .map_err(|e| AppError::Internal(format!("redis refresh token error: {e}")))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn revoke_refresh_token(&mut self, key: &str) -> AppResult<()> {
        self.invalidate(key).await
    }
}
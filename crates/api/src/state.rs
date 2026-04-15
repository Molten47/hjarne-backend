use cache::CacheClient;
use db::PgPool;
use dashmap::DashMap;
use tokio::sync::broadcast;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

// maps auth_user_id → broadcast sender
// every connected WS client gets a receiver from their sender
pub type NotificationHub = DashMap<Uuid, broadcast::Sender<NotificationEvent>>;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cache: CacheClient,
    pub config: Config,
    pub hub: std::sync::Arc<NotificationHub>,
}

impl AppState {
    pub fn new(db: PgPool, cache: CacheClient, config: Config) -> Self {
        Self {
            db,
            cache,
            config,
            hub: std::sync::Arc::new(DashMap::new()),
        }
    }
}
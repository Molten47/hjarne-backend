use serde_json::Value;
use uuid::Uuid;
use tracing::warn;
use crate::state::{AppState, NotificationEvent};

pub async fn push_notification(
    state: &AppState,
    recipient_id: Uuid,
    event_type: &str,
    payload: Value,
) {
    // persist to DB
    let result = sqlx::query_unchecked!(
        r#"
        INSERT INTO notifications (recipient_id, channel, event_type, payload, status)
        VALUES ($1, 'websocket', $2, $3, 'pending')
        "#,
        recipient_id,
        event_type,
        payload
    )
    .execute(&state.db)
    .await;

    if let Err(e) = result {
        warn!("failed to persist notification: {e}");
        return;
    }

    // broadcast to connected WS client if online
    if let Some(sender) = state.hub.get(&recipient_id) {
        let event = NotificationEvent {
            event_type: event_type.to_string(),
            payload,
        };
        let _ = sender.send(event);
    }
}

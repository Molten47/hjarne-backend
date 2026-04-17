use axum::{
    extract::{State, WebSocketUpgrade, Query},
    response::Response,
    extract::ws::{WebSocket, Message},
};
use serde::Deserialize;
use uuid::Uuid;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::state::{AppState, NotificationEvent};
use auth::verify_access_token;

#[derive(Deserialize)]
pub struct WsQuery {
    pub token: String,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    State(state): State<AppState>,
) -> Response {
    // verify JWT before upgrading
    let claims = match verify_access_token(&params.token, &state.config.jwt_public_key) {
        Ok(c) => c,
        Err(_) => {
            warn!("ws: rejected connection — invalid token");
            return axum::response::IntoResponse::into_response(
                (axum::http::StatusCode::UNAUTHORIZED, "invalid token")
            );
        }
    };

   let user_id: Uuid = claims.sub;

    ws.on_upgrade(move |socket| handle_socket(socket, state, user_id))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, user_id: Uuid) {
    // register in hub — get a receiver
    let rx = {
        let sender = state.hub
            .entry(user_id)
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(32);
                tx
            });
        sender.subscribe()
    };

    info!("ws: user {} connected", user_id);

    deliver_pending(&state, user_id, &mut socket).await;

    listen(rx, &mut socket).await;

    // clean up — remove from hub if no other receivers
    if let Some(entry) = state.hub.get(&user_id) {
        if entry.receiver_count() == 0 {
            drop(entry);
            state.hub.remove(&user_id);
        }
    }

    info!("ws: user {} disconnected", user_id);
}

// push any undelivered notifications from DB on connect
async fn deliver_pending(state: &AppState, user_id: Uuid, socket: &mut WebSocket) {
    let auth_user = sqlx::query_unchecked!(
        "SELECT id FROM auth_users WHERE id = $1",
        user_id
    )
    .fetch_optional(&state.db)
    .await;

    let auth_id = match auth_user {
        Ok(Some(row)) => row.id,
        _ => return,
    };

    let pending = sqlx::query_unchecked!(
        r#"
        SELECT id, event_type, payload
        FROM notifications
        WHERE recipient_id = $1
          AND status IN ('pending', 'delivered')
        ORDER BY created_at DESC
        LIMIT 20
        "#,
        auth_id
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    for row in pending {
        let event = NotificationEvent {
            event_type: row.event_type,
            payload: row.payload,
        };
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = socket.send(Message::Text(json.into())).await;
        }

        // mark as delivered
        let _ = sqlx::query_unchecked!(
            "UPDATE notifications SET status = 'delivered', delivered_at = NOW() WHERE id = $1",
            row.id
        )
        .execute(&state.db)
        .await;
    }
}

// forward broadcast events to the socket
async fn listen(
    mut rx: broadcast::Receiver<NotificationEvent>,
    socket: &mut WebSocket,
) {
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(evt) => {
                        if let Ok(json) = serde_json::to_string(&evt) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(p))) => {
                        let _ = socket.send(Message::Pong(p)).await;
                    }
                    _ => {}
                }
            }
        }
    }
}

//! Live updates: one broadcast channel per household. Handlers call `notify` after a
//! change; connected apps receive a short event and reload what they show.
//!
//! WebSocket auth: browsers cannot set headers on WebSockets, so the access token comes
//! as `?token=`. It is checked (signature, session, membership) before the upgrade.

use std::collections::HashMap;
use std::sync::Mutex;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::Response;
use serde::Deserialize;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::AppState;
use crate::auth::decode_access_token;
use crate::error::{AppError, AppResult};

#[derive(Default)]
pub struct Hub {
    channels: Mutex<HashMap<Uuid, broadcast::Sender<String>>>,
}

impl Hub {
    pub fn subscribe(&self, household: Uuid) -> broadcast::Receiver<String> {
        let mut map = self.channels.lock().expect("hub poisoned");
        map.entry(household).or_insert_with(|| broadcast::channel(64).0).subscribe()
    }

    /// Sends `event` (JSON) to everyone connected to the household. No listeners: no-op.
    pub fn notify(&self, household: Uuid, event: &str) {
        let mut map = self.channels.lock().expect("hub poisoned");
        if let Some(tx) = map.get(&household)
            && tx.send(event.to_string()).is_err()
        {
            // Nobody listens any more: drop the channel.
            map.remove(&household);
        }
    }
}

#[derive(Deserialize)]
pub struct LiveQuery {
    pub token: String,
}

pub async fn live(
    State(state): State<AppState>,
    Path(household_id): Path<Uuid>,
    Query(q): Query<LiveQuery>,
    ws: WebSocketUpgrade,
) -> AppResult<Response> {
    let claims = decode_access_token(&state.auth, &q.token).ok_or(AppError::Unauthorized)?;
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sessions s JOIN memberships m ON m.user_id = s.user_id
                        WHERE s.id = $1 AND s.user_id = $2 AND s.revoked_at IS NULL AND s.expires_at > now()
                          AND m.household_id = $3 AND m.active)",
    )
    .bind(claims.sid)
    .bind(claims.sub)
    .bind(household_id)
    .fetch_one(&state.db)
    .await?;
    if !allowed {
        return Err(AppError::Unauthorized);
    }
    let rx = state.live.subscribe(household_id);
    Ok(ws.on_upgrade(move |socket| forward(socket, rx)))
}

async fn forward(mut socket: WebSocket, mut rx: broadcast::Receiver<String>) {
    if socket.send(Message::Text(r#"{"type":"hello"}"#.into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(text) => if socket.send(Message::Text(text.into())).await.is_err() { break },
                // Missed events: tell the app to reload everything.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if socket.send(Message::Text(r#"{"type":"tasks_changed"}"#.into())).await.is_err() { break }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {} // pings are answered by axum; other client messages are ignored
            },
        }
    }
}

use axum::extract::ws::{Message, WebSocket};
use smql_hooks::EventBus;
use std::sync::Arc;

/// Query parameters for WebSocket event filtering.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct SubscribeParams {
    pub machine: Option<String>,
    pub event: Option<String>,
}

/// Handle a WebSocket connection, forwarding matching EventBus events as JSON.
pub async fn handle_ws(mut socket: WebSocket, event_bus: Arc<EventBus>, params: SubscribeParams) {
    let mut receiver = event_bus.subscribe();

    loop {
        tokio::select! {
            result = receiver.recv() => {
                match result {
                    Ok(event) => {
                        // Apply filters
                        if let Some(ref machine) = params.machine {
                            if event.machine != *machine {
                                continue;
                            }
                        }
                        if let Some(ref event_name) = params.event {
                            if event.name != *event_name {
                                continue;
                            }
                        }

                        let payload = serde_json::json!({
                            "event": event.name,
                            "instance_id": event.instance_id,
                            "machine": event.machine,
                            "payload": event.payload.map(|v| crate::handlers::value_to_json(&v)),
                        });

                        if socket
                            .send(Message::Text(payload.to_string().into()))
                            .await
                            .is_err()
                        {
                            break; // Client disconnected
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "WebSocket subscriber lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break; // EventBus shut down
                    }
                }
            }
            // Check for client disconnect (incoming messages)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // Ignore other messages from client
                }
            }
        }
    }
}

use crate::error::{SdkError, SdkResult};
use crate::types::SdkEvent;
use futures_util::StreamExt;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

/// A WebSocket subscription to server events.
pub struct Subscription {
    ws_stream: WsStream,
}

impl Subscription {
    /// Connect to the WebSocket endpoint.
    pub(crate) async fn connect(url: &str) -> SdkResult<Self> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| SdkError::Subscription(e.to_string()))?;
        Ok(Self { ws_stream })
    }

    /// Receive the next event (blocks until available).
    pub async fn next_event(&mut self) -> SdkResult<SdkEvent> {
        loop {
            match self.ws_stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    let event: SdkEvent = serde_json::from_str(&text)
                        .map_err(|e| SdkError::Deserialize(e.to_string()))?;
                    return Ok(event);
                }
                Some(Ok(Message::Close(_))) => {
                    return Err(SdkError::Subscription("Connection closed".to_string()));
                }
                Some(Ok(_)) => continue, // skip ping/pong/binary
                Some(Err(e)) => {
                    return Err(SdkError::Subscription(e.to_string()));
                }
                None => {
                    return Err(SdkError::Subscription("Stream ended".to_string()));
                }
            }
        }
    }

    /// Spawn a background task that calls the callback for each event.
    /// Returns a handle that can cancel the subscription.
    pub fn on_event<F>(self, callback: F) -> SubscriptionHandle
    where
        F: Fn(SdkEvent) + Send + Sync + 'static,
    {
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        let mut ws = self.ws_stream;

        tokio::spawn(async move {
            tokio::select! {
                _ = cancel_rx => {}
                _ = async {
                    while let Some(Ok(msg)) = ws.next().await {
                        if let Message::Text(text) = msg {
                            if let Ok(event) = serde_json::from_str::<SdkEvent>(&text) {
                                callback(event);
                            }
                        }
                    }
                } => {}
            }
        });

        SubscriptionHandle { cancel_tx: Some(cancel_tx) }
    }
}

/// Handle for cancelling a background subscription.
pub struct SubscriptionHandle {
    cancel_tx: Option<oneshot::Sender<()>>,
}

impl SubscriptionHandle {
    /// Cancel the subscription.
    pub fn cancel(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

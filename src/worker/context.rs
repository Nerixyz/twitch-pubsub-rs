use std::{str::FromStr, time::Duration};

use fastwebsockets::{CloseCode, Frame, OpCode, Payload};
use tokio::sync::mpsc;
use tracing::{debug, warn};
use url::Url;

use crate::{handler::WorkerEvent, ws};

pub struct WsContext<E> {
    id: usize,
    sock: ws::Socket,
    timeout: Duration,
    tx: mpsc::Sender<WorkerEvent<E>>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum WsReadError<E> {
    #[error("Read timed out")]
    Timeout,
    #[error("Failed to parse WebSocket message")]
    Parse,
    #[error("WebSocket message wasn't text")]
    NotText,
    #[error("WebSocket message wasn't valid UTF8")]
    NotUtf8,
    #[error("Failed to deserialize message: {0}")]
    Deserialize(E),
}

impl<E> WsContext<E> {
    pub(crate) fn new(id: usize, sock: ws::Socket, tx: mpsc::Sender<WorkerEvent<E>>) -> Self {
        Self {
            id,
            sock,
            timeout: Duration::from_secs(60),
            tx,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub async fn reconnect(&mut self, url: &Url) -> Result<(), ws::StartError> {
        debug!("Reconnect");
        let _ = self
            .sock
            .write_frame(Frame::close(CloseCode::Normal.into(), b""))
            .await;
        self.sock = ws::connect(url).await?;
        Ok(())
    }

    pub(crate) async fn read<M>(&mut self) -> Result<M, WsReadError<M::Err>>
    where
        M: FromStr,
    {
        let frame = tokio::time::timeout(self.timeout, self.sock.read_frame()).await;
        let Ok(frame) = frame else {
            warn!("Read timed out");
            let _ = self
                .sock
                .write_frame(Frame::close(CloseCode::Normal.into(), b""))
                .await;
            return Err(WsReadError::Timeout);
        };

        let msg = match frame {
            Ok(msg) => msg,
            Err(error) => {
                warn!(%error, "Failed to parse websocket message");
                let _ = self
                    .sock
                    .write_frame(Frame::close(CloseCode::Normal.into(), b""))
                    .await;
                return Err(WsReadError::Parse);
            }
        };
        if msg.opcode != OpCode::Text {
            return Err(WsReadError::NotText);
        }

        if !msg.is_utf8() {
            warn!("Received non-UTF8 message");
            return Err(WsReadError::NotUtf8);
        }
        // TODO: fix the lib or smth
        let utf8 = unsafe { std::str::from_utf8_unchecked(&msg.payload) };
        utf8.parse::<M>().map_err(WsReadError::Deserialize)
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub async fn emit(&mut self, event: E) -> Result<(), mpsc::error::SendError<WorkerEvent<E>>> {
        self.tx.send(WorkerEvent::User(self.id, event)).await
    }

    pub async fn send(&mut self, msg: &str) -> Result<(), fastwebsockets::WebSocketError> {
        self.sock
            .write_frame(Frame::text(Payload::Borrowed(msg.as_bytes())))
            .await
    }

    pub(crate) async fn close(mut self) {
        let _ = self
            .sock
            .write_frame(Frame::close(CloseCode::Normal.into(), b""))
            .await;
        let _ = self.tx.send(WorkerEvent::Closed(self.id)).await;
    }
}

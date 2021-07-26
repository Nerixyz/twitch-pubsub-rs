use std::sync::Arc;

use async_tungstenite::tungstenite::error::Error as WsError;
use tokio::sync::oneshot::Sender;

use super::state::*;
use crate::{connection::transport::WsStreamHalves, Error, ServerMessage, TokenProvider};

pub struct ConnectionLoopClosedState<T: TokenProvider> {
    pub(crate) cause: Error<T>,
}

impl<T: TokenProvider> ConnectionLoopStateFunctions<T> for ConnectionLoopClosedState<T> {
    fn send_message(&mut self, _: String, callback: Option<Sender<Result<(), Error<T>>>>) {
        if let Some(callback) = callback {
            callback.send(Err(self.cause.clone())).ok();
        }
    }

    fn on_transport_init_finished(
        self,
        _: Result<WsStreamHalves<T>, Error<T>>,
    ) -> ConnectionLoopState<T> {
        ConnectionLoopState::Closed(self)
    }

    fn on_send_error(self, _: Arc<WsError>) -> ConnectionLoopState<T> {
        ConnectionLoopState::Closed(self)
    }

    fn on_incoming_message(
        self,
        _: Option<Result<ServerMessage, Error<T>>>,
    ) -> ConnectionLoopState<T> {
        ConnectionLoopState::Closed(self)
    }

    fn send_ping(&mut self) {}

    fn check_pong(self) -> ConnectionLoopState<T> {
        ConnectionLoopState::Closed(self)
    }
}

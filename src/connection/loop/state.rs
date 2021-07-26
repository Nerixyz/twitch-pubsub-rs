use super::{closed, initializing, open};
use crate::{connection::transport::WsStreamHalves, Error, ServerMessage, TokenProvider};
use async_tungstenite::tungstenite::Error as WsError;
use enum_dispatch::enum_dispatch;
use std::sync::Arc;
use tokio::sync::oneshot;

#[enum_dispatch]
pub trait ConnectionLoopStateFunctions<T: TokenProvider> {
    fn send_message(
        &mut self,
        message: String,
        reply_sender: Option<oneshot::Sender<Result<(), Error<T>>>>,
    );

    fn on_transport_init_finished(
        self,
        init_result: Result<WsStreamHalves<T>, Error<T>>,
    ) -> ConnectionLoopState<T>;

    fn on_send_error(self, error: Arc<WsError>) -> ConnectionLoopState<T>;

    fn on_incoming_message(
        self,
        maybe_message: Option<Result<ServerMessage, Error<T>>>,
    ) -> ConnectionLoopState<T>;

    fn send_ping(&mut self);
    fn check_pong(self) -> ConnectionLoopState<T>;
}

#[enum_dispatch(ConnectionLoopStateFunctions<T>)]
pub enum ConnectionLoopState<T: TokenProvider> {
    Initializing(initializing::ConnectionLoopInitializingState<T>),
    Open(open::ConnectionLoopOpenState<T>),
    Closed(closed::ConnectionLoopClosedState<T>),
}

use std::sync::Arc;

use async_tungstenite::tungstenite::error::Error as WsError;
use tokio::sync::{mpsc, oneshot};

use super::{closed, state::*};
use crate::{
    connection::{
        message::ConnectionLoopMessage,
        transport::{WsStreamHalves, WsStreamItem},
    },
    Error, TokenProvider,
};
use twitch_api2::pubsub::Response;

type MessageSender<T> =
    mpsc::UnboundedSender<(String, Option<oneshot::Sender<Result<(), Error<T>>>>)>;

pub struct ConnectionLoopOpenState<T: TokenProvider> {
    pub(crate) connection_incoming_tx: mpsc::UnboundedSender<ConnectionLoopMessage<T>>,
    pub(crate) outgoing_messages_tx: MessageSender<T>,

    pub(crate) pong_received: bool,

    pub(crate) kill_incoming_loop_tx: Option<oneshot::Sender<()>>,
    pub(crate) kill_pinger_tx: Option<oneshot::Sender<()>>,
}

impl<T: TokenProvider> ConnectionLoopOpenState<T> {
    fn transition_to_closed(self, cause: Error<T>) -> ConnectionLoopState<T> {
        log::info!("Closing connection, reason: {}", cause);
        self.connection_incoming_tx
            .send(ConnectionLoopMessage::Closed {
                cause: cause.clone(),
            })
            .ok();

        ConnectionLoopState::Closed(closed::ConnectionLoopClosedState { cause })
    }
}

impl<T: TokenProvider> Drop for ConnectionLoopOpenState<T> {
    fn drop(&mut self) {
        self.kill_incoming_loop_tx.take().unwrap().send(()).ok();
        self.kill_pinger_tx.take().unwrap().send(()).ok();
    }
}

impl<T: TokenProvider> ConnectionLoopStateFunctions<T> for ConnectionLoopOpenState<T> {
    fn send_message(
        &mut self,
        message: String,
        callback: Option<oneshot::Sender<Result<(), Error<T>>>>,
    ) {
        self.outgoing_messages_tx.send((message, callback)).ok();
    }

    fn on_transport_init_finished(
        self,
        _: Result<WsStreamHalves<T>, Error<T>>,
    ) -> ConnectionLoopState<T> {
        unreachable!("transport init cannot finish more than once")
    }

    fn on_send_error(self, error: Arc<WsError>) -> ConnectionLoopState<T> {
        self.transition_to_closed(Error::OutgoingError(error))
    }

    fn on_incoming_message(
        mut self,
        maybe_message: Option<WsStreamItem<T>>,
    ) -> ConnectionLoopState<T> {
        match maybe_message {
            None => {
                log::info!("EOF from incoming WebSocket stream");
                return self.transition_to_closed(Error::RemoteUnexpectedlyClosedConnection);
            }
            Some(Err(error)) => {
                log::error!("Error from incoming WebSocket stream: {}", error);
                return self.transition_to_closed(error);
            }
            Some(Ok(response)) => {
                let mut should_reconnect = false;
                match &response {
                    Ok(Response::Pong) => {
                        self.pong_received = true;
                    }
                    Ok(Response::Reconnect) => should_reconnect = true,
                    _ => (),
                }
                self.connection_incoming_tx
                    .send(ConnectionLoopMessage::ServerMessage(response))
                    .ok();

                if should_reconnect {
                    return self.transition_to_closed(Error::ReconnectMessage);
                }
            }
        }

        ConnectionLoopState::Open(self)
    }

    fn send_ping(&mut self) {
        self.pong_received = false;
        self.send_message(serde_json::json!({ "type": "PING" }).to_string(), None);
    }

    fn check_pong(self) -> ConnectionLoopState<T> {
        if !self.pong_received {
            self.transition_to_closed(Error::PingTimeout)
        } else {
            ConnectionLoopState::Open(self)
        }
    }
}

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use async_tungstenite::tungstenite::error::Error as WsError;
use futures::{SinkExt, StreamExt};
use tokio::{
    sync::{mpsc, oneshot},
    time::{interval_at, Instant},
};

use super::{closed, open, state::*};
use crate::{
    connection::{
        message::{ConnectionLoopCommand, ConnectionLoopMessage},
        transport::{WsSink, WsStream, WsStreamHalves},
    },
    Error, ServerMessage, TokenProvider,
};
use std::collections::VecDeque;

type CommandQueue<T> = VecDeque<(String, Option<oneshot::Sender<Result<(), Error<T>>>>)>;
type MessageReceiver<T> =
    mpsc::UnboundedReceiver<(String, Option<oneshot::Sender<Result<(), Error<T>>>>)>;

pub struct ConnectionLoopInitializingState<T: TokenProvider> {
    // a list of queued up ConnectionLoopCommand::SendMessage messages
    pub(crate) commands_queue: CommandQueue<T>,
    pub(crate) connection_loop_tx: Weak<mpsc::UnboundedSender<ConnectionLoopCommand<T>>>,
    pub(crate) connection_incoming_tx: mpsc::UnboundedSender<ConnectionLoopMessage<T>>,
}

impl<T: TokenProvider> ConnectionLoopInitializingState<T> {
    fn transition_to_closed(self, error: Error<T>) -> ConnectionLoopState<T> {
        log::info!("Closing connection, reason: {}", error);
        for callback in self.commands_queue.into_iter().filter_map(|q| q.1) {
            callback.send(Err(error.clone())).ok();
        }

        self.connection_incoming_tx
            .send(ConnectionLoopMessage::Closed {
                cause: error.clone(),
            })
            .ok();

        ConnectionLoopState::Closed(closed::ConnectionLoopClosedState { cause: error })
    }

    async fn forward_incoming_messages(
        mut incoming: WsStream<T>,
        connection_loop_tx: Weak<mpsc::UnboundedSender<ConnectionLoopCommand<T>>>,
        mut shutdown_notify: oneshot::Receiver<()>,
    ) {
        log::debug!("Started forwarding incoming messaged to connection loop");
        loop {
            tokio::select! {
                _ = &mut shutdown_notify => {
                    break;
                }
                msg = incoming.next() => {
                    let should_exit = matches!(msg, None | Some(Err(_)));

                    if let Some(loop_tx) = connection_loop_tx.upgrade() {
                        loop_tx.send(ConnectionLoopCommand::IncomingMessage(msg)).ok();
                    } else {
                        break;
                    }

                    if should_exit {
                        break;
                    }
                }
            }
        }
        log::debug!("Stopped forwarding incoming messaged");
    }

    async fn forward_outgoing_messages(
        mut outgoing: WsSink,
        mut messages_rx: MessageReceiver<T>,
        connection_loop_tx: Weak<mpsc::UnboundedSender<ConnectionLoopCommand<T>>>,
    ) {
        log::debug!("Started forwarding outgoing messages to transport");
        while let Some((message, callback)) = messages_rx.recv().await {
            let res = outgoing.send(message).await.map_err(Arc::new);
            if let Err(ref err) = res {
                if let Some(loop_tx) = connection_loop_tx.upgrade() {
                    loop_tx
                        .send(ConnectionLoopCommand::SendErr(Arc::clone(err)))
                        .ok();
                }
            }

            if let Some(callback) = callback {
                callback.send(res.map_err(Error::WsError)).ok();
            }
        }
        log::debug!("Stopped forwarding outgoing messages to transport");
    }

    async fn ping_task(
        connection_loop_tx: Weak<mpsc::UnboundedSender<ConnectionLoopCommand<T>>>,
        mut shutdown_notify: oneshot::Receiver<()>,
    ) {
        log::debug!("Started ping task");
        let ping_interval = Duration::from_secs(60);
        let check_pong_after = Duration::from_secs(5);

        let mut send_ping_interval = interval_at(Instant::now() + ping_interval, ping_interval);
        let mut check_pong_interval = interval_at(
            Instant::now() + ping_interval + check_pong_after,
            ping_interval,
        );

        loop {
            tokio::select! {
                _ = &mut shutdown_notify => {
                    break;
                }
                _ = send_ping_interval.tick() => {
                    if let Some(loop_tx) = connection_loop_tx.upgrade() {
                        loop_tx.send(ConnectionLoopCommand::SendPing).ok();
                    } else {
                        break;
                    }
                }
                _ = check_pong_interval.tick() => {
                    if let Some(loop_tx) = connection_loop_tx.upgrade() {
                        loop_tx.send(ConnectionLoopCommand::CheckPong).ok();
                    } else {
                        break;
                    }
                }
            }
        }
        log::debug!("Stopped ping task");
    }
}

impl<T: TokenProvider> ConnectionLoopStateFunctions<T> for ConnectionLoopInitializingState<T> {
    fn send_message(
        &mut self,
        message: String,
        reply_sender: Option<oneshot::Sender<Result<(), Error<T>>>>,
    ) {
        self.commands_queue.push_back((message, reply_sender));
    }

    fn on_transport_init_finished(
        self,
        init_result: Result<WsStreamHalves<T>, Error<T>>,
    ) -> ConnectionLoopState<T> {
        let transport = match init_result {
            Ok(transport) => transport,
            Err(e) => {
                log::error!("Initializing transport failed - closing - reason: {}", e);
                return self.transition_to_closed(e);
            }
        };
        log::debug!("Transport initialized, transitioning to OpenState");

        let incoming = transport.read;
        let outgoing = transport.write;

        let (kill_incoming_loop_tx, kill_incoming_loop_rx) = oneshot::channel();
        tokio::spawn(Self::forward_incoming_messages(
            incoming.into(),
            Weak::clone(&self.connection_loop_tx),
            kill_incoming_loop_rx,
        ));

        let (outgoing_messages_tx, outgoing_messages_rx) = mpsc::unbounded_channel();
        tokio::spawn(Self::forward_outgoing_messages(
            outgoing.into(),
            outgoing_messages_rx,
            Weak::clone(&self.connection_loop_tx),
        ));

        let (kill_pinger_tx, kill_pinger_rx) = oneshot::channel();
        tokio::spawn(Self::ping_task(
            Weak::clone(&self.connection_loop_tx),
            kill_pinger_rx,
        ));

        self.connection_incoming_tx
            .send(ConnectionLoopMessage::Open)
            .ok();

        let mut new_state = ConnectionLoopState::Open(open::ConnectionLoopOpenState {
            connection_incoming_tx: self.connection_incoming_tx,
            outgoing_messages_tx,
            pong_received: false,
            kill_incoming_loop_tx: Some(kill_incoming_loop_tx),
            kill_pinger_tx: Some(kill_pinger_tx),
        });

        for (message, callback) in self.commands_queue.into_iter() {
            new_state.send_message(message, callback);
        }

        new_state
    }

    fn on_send_error(self, error: Arc<WsError>) -> ConnectionLoopState<T> {
        self.transition_to_closed(Error::OutgoingError(error))
    }

    fn on_incoming_message(
        self,
        _: Option<Result<ServerMessage, Error<T>>>,
    ) -> ConnectionLoopState<T> {
        unreachable!("Can't receive messages while initializing")
    }

    fn send_ping(&mut self) {
        unreachable!("Can't receive messages while initializing")
    }

    fn check_pong(self) -> ConnectionLoopState<T> {
        unreachable!("Can't receive messages while initializing")
    }
}

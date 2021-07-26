use std::{
    collections::VecDeque,
    sync::{Arc, Weak},
};
use tokio::sync::mpsc;

use crate::{
    connection::{
        message::{ConnectionLoopCommand, ConnectionLoopMessage},
        transport::{connect_to_pubsub, WsStreamHalves},
    },
    ClientConfig, Error, TokenProvider,
};
use state::*;

pub mod closed;
pub mod initializing;
pub mod open;
pub mod state;

pub(crate) struct ConnectionLoopWorker<T: TokenProvider> {
    connection_loop_rx: mpsc::UnboundedReceiver<ConnectionLoopCommand<T>>,
    state: ConnectionLoopState<T>,
}

impl<T: TokenProvider> ConnectionLoopWorker<T> {
    pub fn spawn(
        config: Arc<ClientConfig<T>>,
        connection_incoming_tx: mpsc::UnboundedSender<ConnectionLoopMessage<T>>,
        connection_loop_tx: Weak<mpsc::UnboundedSender<ConnectionLoopCommand<T>>>,
        connection_loop_rx: mpsc::UnboundedReceiver<ConnectionLoopCommand<T>>,
    ) {
        let worker = ConnectionLoopWorker {
            connection_loop_rx,
            state: ConnectionLoopState::Initializing(
                initializing::ConnectionLoopInitializingState {
                    commands_queue: VecDeque::new(),
                    connection_loop_tx: Weak::clone(&connection_loop_tx),
                    connection_incoming_tx,
                },
            ),
        };

        tokio::spawn(Self::run_init_task(config, connection_loop_tx));
        tokio::spawn(worker.run_loop());
    }

    async fn run_init_task(
        config: Arc<ClientConfig<T>>,
        connection_loop_tx: Weak<mpsc::UnboundedSender<ConnectionLoopCommand<T>>>,
    ) {
        log::debug!("Started init task - creating a new transport");
        let res = Self::create_new_transport(config).await;

        if let Some(loop_tx) = connection_loop_tx.upgrade() {
            loop_tx
                .send(ConnectionLoopCommand::WsInitFinished(res))
                .ok();
        }
    }

    async fn create_new_transport(
        config: Arc<ClientConfig<T>>,
    ) -> Result<WsStreamHalves<T>, Error<T>> {
        // rate limits the opening of new connections
        log::trace!("Trying to acquire permit for opening transport...");
        let rate_limit_permit = Arc::clone(&config.connection_rate_limiter)
            .acquire_owned()
            .await
            .map_err(|_| Error::Unknown)?;
        log::trace!("Successfully got permit to open transport.");

        let stream = connect_to_pubsub(&*config).await?;

        tokio::spawn(async move {
            tokio::time::sleep(config.new_connection_every).await;
            drop(rate_limit_permit);
            log::trace!("Released permit after waiting specified duration.");
        });

        Ok(stream)
    }

    async fn run_loop(mut self) {
        log::debug!("Started main connection loop");
        while let Some(cmd) = self.connection_loop_rx.recv().await {
            self = self.process_command(cmd);
        }
        log::debug!("Connection loop ended");
    }

    fn process_command(mut self, command: ConnectionLoopCommand<T>) -> Self {
        match command {
            ConnectionLoopCommand::WsInitFinished(res) => {
                self.state = self.state.on_transport_init_finished(res)
            }
            ConnectionLoopCommand::SendMessage { message, callback } => {
                self.state.send_message(message, callback);
            }
            ConnectionLoopCommand::SendPing => {
                self.state.send_ping();
            }
            ConnectionLoopCommand::CheckPong => {
                self.state = self.state.check_pong();
            }
            ConnectionLoopCommand::IncomingMessage(msg) => {
                self.state = self.state.on_incoming_message(msg);
            }
            ConnectionLoopCommand::SendErr(e) => {
                self.state = self.state.on_send_error(e);
            }
        }
        self
    }
}

use crate::{
    client::{messages::ClientLoopCommand, pool_connection::PoolConnection},
    connection::{
        message::{ConnectionLoopCommand, ConnectionLoopMessage},
        Connection,
    },
    util::generate_nonce,
    ClientConfig, Error, ServerMessage, TokenProvider, Topic, TwitchResponse,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Weak},
};
use tokio::sync::{mpsc, oneshot};

pub(crate) struct ClientLoopWorker<T: TokenProvider> {
    config: Arc<ClientConfig<T>>,
    next_connection_id: usize,
    client_loop_rx: mpsc::UnboundedReceiver<ClientLoopCommand<T>>,
    connections: VecDeque<PoolConnection<T>>,
    client_loop_tx: Weak<mpsc::UnboundedSender<ClientLoopCommand<T>>>,
    client_incoming_messages_tx: mpsc::UnboundedSender<ServerMessage>,
}

impl<T: TokenProvider> ClientLoopWorker<T> {
    pub fn spawn(
        config: Arc<ClientConfig<T>>,
        client_loop_tx: Weak<mpsc::UnboundedSender<ClientLoopCommand<T>>>,
        client_loop_rx: mpsc::UnboundedReceiver<ClientLoopCommand<T>>,
        client_incoming_messages_tx: mpsc::UnboundedSender<ServerMessage>,
    ) {
        let worker = ClientLoopWorker {
            config,
            next_connection_id: 0,
            client_loop_rx,
            client_loop_tx,
            connections: VecDeque::new(),
            client_incoming_messages_tx,
        };

        tokio::spawn(worker.run());
    }

    async fn run(mut self) {
        log::debug!("Started main client loop");
        while let Some(command) = self.client_loop_rx.recv().await {
            self.process_command(command);
        }
        log::debug!("Stopped main client loop");
    }

    fn process_command(&mut self, command: ClientLoopCommand<T>) {
        match command {
            // ClientLoopCommand::Connect { callback } => {
            //     if self.connections.is_empty() {
            //         let new_connection = self.make_new_connection();
            //         self.connections.push_back(new_connection);
            //     }
            //     callback.send(()).ok();
            // }
            ClientLoopCommand::Listen {
                callback,
                topics,
                token,
            } => {
                self.listen(topics, token, callback);
            }
            ClientLoopCommand::Unlisten { topic, callback } => {
                self.unlisten(topic, callback);
            }

            ClientLoopCommand::IncomingMessage {
                source_connection_id,
                message,
            } => {
                self.on_incoming_message(source_connection_id, message);
            } // ClientLoopCommand::AvailableConnection(conn) => {
              //     self.connections.push_back(conn)
              // }
        }
    }

    fn make_new_connection(&mut self) -> PoolConnection<T> {
        let (connection_incoming_messages_rx, connection) =
            Connection::new(Arc::clone(&self.config));
        let (tx_kill_incoming, rx_kill_incoming) = oneshot::channel();

        let connection_id = self.next_connection_id;
        self.next_connection_id = self.next_connection_id.overflowing_add(1).0;

        log::info!("Making new connection (id={})", connection_id);

        let pool_connection = PoolConnection::new(
            Arc::clone(&self.config),
            connection_id,
            connection,
            tx_kill_incoming,
        );

        tokio::spawn(Self::forward_task(
            connection_incoming_messages_rx,
            connection_id,
            self.client_loop_tx.clone(),
            rx_kill_incoming,
        ));

        pool_connection
    }

    /// Forwards connection messages to the client loop
    /// `[connection loop] -> [client loop]`
    async fn forward_task(
        mut connection_incoming_messages_rx: mpsc::UnboundedReceiver<ConnectionLoopMessage<T>>,
        connection_id: usize,
        client_loop_tx: Weak<mpsc::UnboundedSender<ClientLoopCommand<T>>>,
        mut rx_kill_incoming: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                _ = &mut rx_kill_incoming => {
                    break;
                }
                msg = connection_incoming_messages_rx.recv() => {
                    if let Some(msg) = msg {
                        if let Some(client_loop_tx) = client_loop_tx.upgrade() {
                            client_loop_tx.send(ClientLoopCommand::IncomingMessage {
                                source_connection_id: connection_id,
                                message: msg
                            }).unwrap();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }

    fn listen(
        &mut self,
        topics: Vec<Topic>,
        token: Option<String>,
        callback: Option<oneshot::Sender<Result<(), Error<T>>>>,
    ) {
        // generate + serialize first so we don't waste a connection
        let nonce = generate_nonce(rand::thread_rng());
        let message =
            match crate::listen_command(&topics, &token.unwrap_or_default(), nonce.as_str()) {
                Ok(msg) => msg,
                Err(e) => {
                    if let Some(callback) = callback {
                        callback.send(Err(Error::SerdeError(Arc::new(e)))).ok();
                    }
                    return;
                }
            };
        log::debug!("Listening to {:?} (nonce={})", topics, nonce);

        let mut pool_connection = self
            .connections
            .iter()
            // find a connection that isn't full and that can accept the new topics
            .position(|c| c.can_handle(topics.len()))
            .map(
                |idx| self.connections.remove(idx).unwrap(), /* we know the index exists */
            )
            // else, make a new connection
            .unwrap_or_else(|| self.make_new_connection());

        pool_connection
            .send(ConnectionLoopCommand::SendMessage {
                message,
                callback: None,
            })
            .unwrap(); // the client loop is active and thus the receiver

        pool_connection.wanted_topics += topics.len();
        pool_connection
            .pending_topics
            .insert(nonce, (Some(topics), callback));

        self.connections.push_back(pool_connection);
    }

    fn unlisten(&mut self, topic: Topic, callback: Option<oneshot::Sender<Result<(), Error<T>>>>) {
        let mut pool_connection = match self
            .connections
            .iter()
            // find a connection that isn't full and that can accept the new topics
            .position(|c| c.active_topics.contains(&topic))
            .map(
                |idx| self.connections.remove(idx).unwrap(), /* we know the index exists */
            ) {
            Some(conn) => conn,
            None => {
                if let Some(callback) = callback {
                    // this is fine, we just won't send anything
                    callback.send(Ok(())).ok();
                }
                return;
            }
        };

        let nonce = generate_nonce(rand::thread_rng());
        log::debug!("Unlistening from {:?} (nonce={})", topic, nonce);

        let message = match crate::unlisten_command(&[topic], nonce.as_str()) {
            Ok(msg) => msg,
            Err(e) => {
                if let Some(callback) = callback {
                    callback.send(Err(Error::SerdeError(Arc::new(e)))).ok();
                }
                self.connections.push_back(pool_connection);
                return;
            }
        };

        pool_connection
            .send(ConnectionLoopCommand::SendMessage {
                message,
                callback: None,
            })
            .unwrap(); // the client loop is active and thus the receiver

        pool_connection.wanted_topics -= 1;
        pool_connection
            .pending_topics
            .insert(nonce, (None, callback));

        self.connections.push_back(pool_connection);
    }

    fn on_incoming_message(
        &mut self,
        source_connection_id: usize,
        message: ConnectionLoopMessage<T>,
    ) {
        match message {
            ConnectionLoopMessage::ServerMessage(msg) => {
                match &msg {
                    // we only care about messages with a nonce
                    ServerMessage::Response(TwitchResponse {
                        nonce: Some(nonce),
                        error,
                    }) => {
                        let conn = self
                            .connections
                            .iter_mut()
                            .find(|c| c.id == source_connection_id)
                            .unwrap();

                        if let Some((topics, callback)) = conn.pending_topics.remove(nonce) {
                            let callback_msg = match error {
                                Some(e) if !e.is_empty() => {
                                    log::warn!("Message with nonce {} failed: {}", nonce, e);
                                    // don't insert the topics
                                    Err(Error::ListenError(e.to_string()))
                                }
                                _ => {
                                    log::debug!("Message with nonce {} was successful", nonce);
                                    // assume there's no error
                                    if let Some(topics) = topics {
                                        for topic in topics {
                                            conn.active_topics.insert(topic);
                                        }
                                    }
                                    Ok(())
                                }
                            };
                            if let Some(callback) = callback {
                                callback.send(callback_msg).ok();
                            }
                        }
                    }
                    ServerMessage::Pong => {
                        // we don't need to send a pong
                        return;
                    }
                    _ => (),
                };
                self.client_incoming_messages_tx.send(msg).ok();
            }
            ConnectionLoopMessage::Open => {
                log::debug!("Pool connection {} is open", source_connection_id);
                // TODO: metrics
            }
            ConnectionLoopMessage::Closed { cause } => {
                log::error!(
                    "Pool connection {} has failed due to error (removing it): {}",
                    source_connection_id,
                    cause
                );

                let mut connection = self
                    .connections
                    .iter()
                    .position(|c| c.id == source_connection_id)
                    .and_then(|idx| self.connections.remove(idx))
                    // this is fine as we should always receive only one error upon which we remove the connection
                    .unwrap();

                for (_nonce, (_topics, callback)) in connection.pending_topics.drain() {
                    if let Some(callback) = callback {
                        callback.send(Err(cause.clone())).ok();
                    }
                }

                if connection.active_topics.is_empty() {
                    log::debug!(
                        "Pool connection {} didn't listen to any topics, ignoring it",
                        source_connection_id,
                    );
                    return;
                }

                log::debug!(
                    "Pool connection {} previously actively listened to {} topics ({:?}), hydrating them with a new token",
                    source_connection_id,
                    connection.active_topics.len(),
                    connection.active_topics
                );

                let client_loop_tx = Weak::clone(&self.client_loop_tx);
                let config = Arc::clone(&self.config);
                tokio::spawn(async move {
                    match config
                        .token_provider
                        .provide_many(connection.active_topics.drain().collect())
                        .await
                    {
                        Ok(tokens) => {
                            if let Some(client_loop_tx) = client_loop_tx.upgrade() {
                                log::debug!(
                                    "Tokens for topics from connection {} hydrated.",
                                    source_connection_id
                                );
                                for (topics, token) in tokens {
                                    client_loop_tx
                                        .send(ClientLoopCommand::Listen {
                                            topics,
                                            token,
                                            callback: None,
                                        })
                                        .ok();
                                }
                            }
                        }
                        Err(e) => {
                            // TODO: provide info about the error
                            log::error!("Failed to hydrate topics with new token after a failed connection (id={}): {}", source_connection_id, e);
                        }
                    }
                });
            }
        }
    }
}

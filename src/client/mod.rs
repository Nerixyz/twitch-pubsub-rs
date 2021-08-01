use crate::{
    client::messages::ClientLoopCommand, ClientConfig, Error, ServerMessage, TokenProvider, Topic,
};
use r#loop::ClientLoopWorker;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

mod r#loop;
mod messages;
mod pool_connection;

/// A send-only handle to listen and unlisten to topics.
pub struct PubsubClient<T: TokenProvider> {
    client_loop_tx: Arc<mpsc::UnboundedSender<ClientLoopCommand<T>>>,
    config: Arc<ClientConfig<T>>,
}

impl<T: TokenProvider> Clone for PubsubClient<T> {
    fn clone(&self) -> Self {
        Self {
            client_loop_tx: self.client_loop_tx.clone(),
            config: self.config.clone(),
        }
    }
}

impl<T: TokenProvider> PubsubClient<T> {
    /// Create a new client.
    ///
    /// This will spawn a task in the background.
    pub fn new(
        config: ClientConfig<T>,
    ) -> (mpsc::UnboundedReceiver<ServerMessage<T>>, PubsubClient<T>) {
        let config = Arc::new(config);
        let (client_loop_tx, client_loop_rx) = mpsc::unbounded_channel();
        let client_loop_tx = Arc::new(client_loop_tx);
        let (client_incoming_messages_tx, client_incoming_messages_rx) = mpsc::unbounded_channel();

        ClientLoopWorker::spawn(
            config.clone(),
            Arc::downgrade(&client_loop_tx),
            client_loop_rx,
            client_incoming_messages_tx,
        );

        (
            client_incoming_messages_rx,
            PubsubClient {
                client_loop_tx,
                config,
            },
        )
    }

    /// Listen to a topic (subscribe).
    ///
    /// The future will be ready if the server confirmed the subscription.
    pub async fn listen(&self, topic: Topic) -> Result<(), Error<T>> {
        let token = self
            .config
            .token_provider
            .provide_token(&topic)
            .await
            .map_err(|e| Error::TokenError(Arc::new(e)))?;
        let (tx, rx) = oneshot::channel();

        self.client_loop_tx
            .send(ClientLoopCommand::Listen {
                topics: vec![topic],
                token,
                callback: Some(tx),
            })
            .map_err(|_| Error::ClientLoopDied)?;

        rx.await.map_err(|_| Error::CallbackDied)?
    }

    /// Unlisten from a topic (unsubscribe).
    ///
    /// The future will be ready if the server confirmed the subscription.
    pub async fn unlisten(&self, topic: Topic) -> Result<(), Error<T>> {
        let (tx, rx) = oneshot::channel();
        self.client_loop_tx
            .send(ClientLoopCommand::Unlisten {
                topic,
                callback: Some(tx),
            })
            .map_err(|_| Error::<T>::ClientLoopDied)
            .map_err(|_| Error::ClientLoopDied)?;

        rx.await.map_err(|_| Error::CallbackDied)?
    }

    /// Listen to many topics at once.
    ///
    /// The future will be ready once the token is ready.
    /// **It will not wait for twitch to confirm the topic!**
    ///
    /// _Note:_ In the future this function _might_ wait for twitch to confirm the topic.
    pub async fn listen_many(&self, topics: Vec<Topic>) -> Result<(), Error<T>> {
        let tokens = self
            .config
            .token_provider
            .provide_many(topics)
            .await
            .map_err(|e| Error::TokenError(Arc::new(e)))?;

        for (topics, token) in tokens {
            if topics.len() > self.config.max_topics_per_connection {
                for topics in topics.chunks(self.config.max_topics_per_connection) {
                    self.client_loop_tx
                        .send(ClientLoopCommand::Listen {
                            topics: topics.to_vec(),
                            token: token.clone(),
                            callback: None,
                        })
                        .map_err(|_| Error::ClientLoopDied)?;
                }
            } else {
                self.client_loop_tx
                    .send(ClientLoopCommand::Listen {
                        topics,
                        token,
                        callback: None,
                    })
                    .map_err(|_| Error::ClientLoopDied)?;
            }
        }

        Ok(())
    }
}

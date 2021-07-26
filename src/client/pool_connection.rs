use crate::{
    connection::{message::ConnectionLoopCommand, Connection},
    ClientConfig, Error, TokenProvider, Topic,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{mpsc::error::SendError, oneshot};

#[derive(Debug)]
pub(crate) struct PoolConnection<T: TokenProvider> {
    config: Arc<ClientConfig<T>>,

    pub id: usize,

    pub connection: Arc<Connection<T>>,

    /// Contains a map of `{ nonce -> topic, callback }`.
    pub pending_topics: HashMap<
        String,
        (
            Option<Vec<Topic>>,
            Option<oneshot::Sender<Result<(), Error<T>>>>,
        ),
    >,
    /// Contains the number of topics the client listens to or wants to listen to.
    pub wanted_topics: usize,
    // Contains all the topics the client definitely listens to.
    pub active_topics: HashSet<Topic>,

    tx_kill_incoming: Option<oneshot::Sender<()>>,
}

impl<T: TokenProvider> PoolConnection<T> {
    pub fn new(
        config: Arc<ClientConfig<T>>,
        id: usize,
        connection: Connection<T>,
        tx_kill_incoming: oneshot::Sender<()>,
    ) -> Self {
        Self {
            config,
            id,
            connection: Arc::new(connection),
            pending_topics: HashMap::new(),
            wanted_topics: 0,
            active_topics: HashSet::new(),
            tx_kill_incoming: Some(tx_kill_incoming),
        }
    }

    pub fn can_handle(&self, n_topics: usize) -> bool {
        self.wanted_topics + n_topics <= self.config.max_topics_per_connection
    }

    pub fn send(
        &self,
        cmd: ConnectionLoopCommand<T>,
    ) -> Result<(), SendError<ConnectionLoopCommand<T>>> {
        self.connection.connection_loop_tx.send(cmd)
    }
}

impl<T: TokenProvider> Drop for PoolConnection<T> {
    fn drop(&mut self) {
        self.tx_kill_incoming.take().unwrap().send(()).ok();
    }
}

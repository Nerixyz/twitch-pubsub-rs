use crate::{
    connection::message::{ConnectionLoopCommand, ConnectionLoopMessage},
    ClientConfig, TokenProvider,
};
use r#loop::ConnectionLoopWorker;
use std::sync::Arc;
use tokio::sync::mpsc;

mod r#loop;
pub mod message;
pub mod transport;

#[derive(Debug)]
pub(crate) struct Connection<T: TokenProvider> {
    pub connection_loop_tx: Arc<mpsc::UnboundedSender<ConnectionLoopCommand<T>>>,
}

impl<T: TokenProvider> Connection<T> {
    pub fn new(
        config: Arc<ClientConfig<T>>,
    ) -> (mpsc::UnboundedReceiver<ConnectionLoopMessage<T>>, Self) {
        let (connection_loop_tx, connection_loop_rx) = mpsc::unbounded_channel();
        let (connection_incoming_tx, connection_incoming_rx) = mpsc::unbounded_channel();
        let connection_loop_tx = Arc::new(connection_loop_tx);

        ConnectionLoopWorker::spawn(
            config,
            connection_incoming_tx,
            Arc::downgrade(&connection_loop_tx),
            connection_loop_rx,
        );

        (connection_incoming_rx, Connection { connection_loop_tx })
    }
}

use crate::{connection::message::ConnectionLoopMessage, Error, TokenProvider, Topic};
use tokio::sync::oneshot;

#[derive(Debug)]
pub(crate) enum ClientLoopCommand<T: TokenProvider> {
    // Connect {
    //     callback: oneshot::Sender<()>,
    // },
    Listen {
        topics: Vec<Topic>,
        token: Option<String>,
        callback: Option<oneshot::Sender<Result<(), Error<T>>>>,
    },
    Unlisten {
        topic: Topic,
        callback: Option<oneshot::Sender<Result<(), Error<T>>>>,
    },

    IncomingMessage {
        source_connection_id: usize,
        message: ConnectionLoopMessage<T>,
    },
    // AvailableConnection(PoolConnection<T>),
}

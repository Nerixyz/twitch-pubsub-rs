use crate::token_provider::TokenProvider;
use async_tungstenite::tungstenite::Error as WsError;
use serde_json::Error as SerdeError;
use std::sync::Arc;

/// All the possible errors returned by the client- and the connection-loop.
#[derive(Debug, thiserror::Error)]
pub enum Error<T: TokenProvider> {
    /// Websocket error
    #[error("Websocket error - {0}")]
    WsError(Arc<WsError>),
    /// Error in the outgoing connection
    #[error("Outgoing error - {0}")]
    OutgoingError(Arc<WsError>),
    /// The remote host unexpectedly closed an open connection
    #[error("The remote host unexpectedly closed an open connection")]
    RemoteUnexpectedlyClosedConnection,
    /// Received a reconnect message
    #[error("Received a reconnect message")]
    ReconnectMessage,
    /// Connection timed out
    #[error("Connection timed out")]
    ConnectTimeout,
    /// A ping timed out
    #[error("A ping timed out")]
    PingTimeout,
    /// Failed listening to some topics (message by twitch)
    #[error("Failed listening to the topics - {0}")]
    ListenError(String),
    /// Error when (de-)serializing
    #[error("Serde error - {0}")]
    SerdeError(Arc<SerdeError>),
    /// Error from the token provider
    #[error("Error providing token - {0}")]
    TokenError(Arc<T::Error>),
    /// The client loop died before sending
    #[error("Client loop died before sending")]
    ClientLoopDied,
    /// The callback died - Sender dropped without sending anything
    #[error("Callback died - Sender dropped without sending anything")]
    CallbackDied,
    /// An unknown error
    #[error("Unknown error")]
    Unknown,
}

impl<T: TokenProvider> Clone for Error<T> {
    fn clone(&self) -> Self {
        match self {
            Error::WsError(e) => Error::WsError(Arc::clone(e)),
            Error::OutgoingError(e) => Error::OutgoingError(Arc::clone(e)),
            Error::RemoteUnexpectedlyClosedConnection => Error::RemoteUnexpectedlyClosedConnection,
            Error::ReconnectMessage => Error::ReconnectMessage,
            Error::PingTimeout => Error::PingTimeout,
            Error::ListenError(e) => Error::ListenError(e.clone()),
            Error::SerdeError(e) => Error::SerdeError(Arc::clone(e)),
            Error::TokenError(e) => Error::TokenError(e.clone()),
            Error::ClientLoopDied => Error::ClientLoopDied,
            Error::CallbackDied => Error::CallbackDied,
            Error::Unknown => Error::Unknown,
            Error::ConnectTimeout => Error::ConnectTimeout,
        }
    }
}

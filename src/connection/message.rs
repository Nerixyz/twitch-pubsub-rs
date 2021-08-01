use crate::{connection::transport::WsStreamHalves, Error, TokenProvider};
use async_tungstenite::tungstenite::Error as WsError;
use std::sync::Arc;
use tokio::sync::oneshot;
use twitch_api2::pubsub::Response;

/// This message comes out of the event-loop
#[derive(Debug)]
pub enum ConnectionLoopMessage<T: TokenProvider> {
    ServerMessage(Result<Response, (String, serde_json::Error)>),
    Open,
    Closed { cause: Error<T> },
}

/// This is a command to the event-loop
#[derive(Debug)]
pub enum ConnectionLoopCommand<T: TokenProvider> {
    SendMessage {
        message: String,
        callback: Option<oneshot::Sender<Result<(), Error<T>>>>,
    },

    WsInitFinished(Result<WsStreamHalves<T>, Error<T>>),
    SendPing,
    CheckPong,

    IncomingMessage(Option<Result<Result<Response, (String, serde_json::Error)>, Error<T>>>),
    SendErr(Arc<WsError>),
}

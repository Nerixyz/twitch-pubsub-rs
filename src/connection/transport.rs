use crate::{ClientConfig, Error, TokenProvider};
use async_tungstenite::{
    tokio::connect_async,
    tungstenite::{Error as WsError, Message},
};
use futures::{future, stream::FusedStream, Sink, SinkExt, StreamExt};
use std::{fmt::Formatter, sync::Arc};
use twitch_api2::pubsub::Response;

pub type WsStreamItem<T> = Result<Result<Response, (String, serde_json::Error)>, Error<T>>;
pub type WsStream<T> = Box<dyn FusedStream<Item = WsStreamItem<T>> + Unpin + Send + Sync>;
pub type WsSink = Box<dyn Sink<String, Error = WsError> + Unpin + Send + Sync>;

pub struct WsStreamHalves<T: TokenProvider> {
    pub read: WsStream<T>,
    pub write: WsSink,
}

impl<T: TokenProvider> std::fmt::Debug for WsStreamHalves<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("WsStreamHalves {}")
    }
}

pub async fn connect_to_pubsub<T: TokenProvider>(
    config: &ClientConfig<T>,
) -> Result<WsStreamHalves<T>, Error<T>> {
    let connection_fut = connect_async(twitch_api2::TWITCH_PUBSUB_URL);
    let (ws_stream, _) = tokio::time::timeout(config.connect_timeout, connection_fut)
        .await
        .map_err(|_| Error::ConnectTimeout)?
        .map_err(|e| Error::WsError(Arc::new(e)))?;

    let (write, read) = ws_stream.split();
    let read = read
        .filter_map(|msg| {
            future::ready(match msg {
                Ok(Message::Text(txt)) => Some(Ok(
                    serde_json::from_str::<Response>(&txt).map_err(|e| (txt, e))
                )),
                Ok(_) => None,
                Err(e) => Some(Err(Error::WsError(Arc::new(e)))),
            })
        })
        .fuse();

    let write = write.with(|msg: String| future::ready(Ok(Message::Text(msg))));

    Ok(WsStreamHalves {
        read: Box::new(read),
        write: Box::new(write),
    })
}

use tokio::sync::mpsc;
use twitch_api::pubsub;
use url::Url;

use crate::{
    pubsub::{PubSubCommand, PubSubEvent, PubSubHandler},
    TokenProvider,
};
use ws_pool::{spawn_handler, HandlerContext};

/// Send handle
pub struct Sender {
    tx: mpsc::UnboundedSender<PubSubCommand>,
}

/// Create a new pubsub manager
/// `url` should be set to `wss://pubsub-edge.twitch.tv` to connect to the public pubsub server.
pub fn create_manager<T: TokenProvider>(
    provider: T,
    url: Url,
) -> (Sender, mpsc::UnboundedReceiver<PubSubEvent<T>>) {
    let (tx, external_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let ctx = HandlerContext::new(url, event_tx, tx.downgrade());

    spawn_handler(PubSubHandler::new(provider), ctx, external_rx);

    (Sender { tx }, event_rx)
}

impl Sender {
    /// Listen to some topics
    pub fn listen(&self, topics: Vec<pubsub::Topics>) -> bool {
        self.tx.send(PubSubCommand::Subscribe(topics)).is_ok()
    }
}

# twitch-pubsub

Connect to Twitch [PubSub](https://dev.twitch.tv/docs/pubsub) from Rust land.

This crate is basically randers' [`twitch-irc-rs`](https://lib.rs/crates/twitch-irc)
but for [PubSub](https://dev.twitch.tv/docs/pubsub).

For the provided types,
this crate is using the types provided by [`twitch_api2`](https://lib.rs/crates/twitch_api2) from Emilgardis.

# Documentation

~~Documentation can be found at [docs.rs](https://docs.rs/twitch-pubsub).~~
Not yet.

# Features

- Automatic reconnection
- Connection pooling
- Custom token handling

# Example

```rust
use twitch_pubsub::{
    PubsubClient,
    Topic,
    moderation,
    providers::StaticTokenProvider,
    ClientConfig,
    ServerMessage,
    TopicData,
};

#[tokio::main]
pub async fn main() {
    let config = ClientConfig::new(StaticTokenProvider::new("MY STATIC SECRET TOKEN"));
    let (mut incoming, client) = PubsubClient::new(config);

    client.listen(Topic::ChatModeratorActions(moderation::ChatModeratorActions {
        // your user-id
        user_id: 129546453,
        channel_id: 129546453
    })).await.expect("Failed listening to chat-moderator-actions");

    while let Some(message) = incoming.recv().await {
        match message {
            ServerMessage::Data(
                TopicData::ChatModeratorActions { topic, reply }
            )=> {
                println!("Message on {:?}: {:?}", topic, reply);
            },
            // handle other messages here
            _ => ()
        }
    }
}
```

#![warn(missing_docs)]
//! Connect to Twitch PubSub from Rust land.
//!
//! # Example
//!
//! ```no_run
//! use twitch_pubsub::{
//!     PubsubClient,
//!     Topic,
//!     moderation,
//!     providers::StaticTokenProvider,
//!     ClientConfig,
//!     ServerMessage,
//!     TopicData,
//! };
//!
//! #[tokio::main]
//! pub async fn main() {
//!     let config = ClientConfig::new(StaticTokenProvider::new("MY STATIC SECRET TOKEN"));
//!     let (mut incoming, client) = PubsubClient::new(config);
//!
//!     client.listen(Topic::ChatModeratorActions(moderation::ChatModeratorActions {
//!         // your user-id
//!         user_id: 129546453,
//!         channel_id: 129546453
//!     })).await.expect("Failed listening to chat-moderator-actions");
//!
//!     while let Some(message) = incoming.recv().await {
//!         match message {
//!             ServerMessage::Message { data: TopicData::ChatModeratorActions { topic, reply } } => {
//!                 println!("Message on {:?}: {:?}", topic, reply);
//!             },
//!             // handle other messages here
//!             _ => ()
//!         }
//!     }
//! }
//! ```
//!
//! # Listening to multiple topics
//!
//! You can listen to multiple topics at once by using `client.listen_many(topics)`.
//! This will use [`TokenProvider.provide_many`](crate::TokenProvider#method.provide_many) under the hood.
//!
//! _Note:_ the returned future will be ready once the tokens are provided.
//! It will not wait for the server to confirm the topic like the `listen` function.
//!
//! # Providing tokens
//!
//! Tokens are provided through the [`TokenProvider`](crate::TokenProvider) trait.
//! Not all topics require a token.
//! The crate provides two simple implementations.
//!
//! * The [`StaticTokenProvider`](crate::providers::StaticTokenProvider) will always provide the same token (enabled by default, feature `static-token-provider`).
//! * The [`NoTokenProvider`](crate::providers::NoTokenProvider) will never provide a token (feature `no-token-provider`).
//!
//! # Features
//!
//! ### Default
//!
//! `static-token-provider` and `native-tls` are enabled by default.
//!
//! ### Providers
//!
//! * `static-token-provider` enables the [`StaticTokenProvider`](crate::providers::StaticTokenProvider).
//! * `no-token-provider` enabled the [`NoTokenProvider`](crate::providers::NoTokenProvider).
//!
//! ### Unsupported
//!
//! * `unsupported` enabled topics that _are not_ documented by Twitch.
//!   Use these at your own risk by understanding the [Twitch Developer Agreement](https://www.twitch.tv/p/en/legal/developer-agreement/).
//!   Changes to these topics may not follow _semver_.
//!   See more at [`twitch_api2`](https://docs.rs/twitch_api2/latest/twitch_api2/pubsub/index.html#undocumented-features).
//!
//! ### TLS
//!
//! * `native-tls` will use the OS-native TLS implementation and the OS-native certificate store.
//! * `rustls-webpki-roots` will use [Rustls](https://github.com/ctz/rustls) as the TLS implementation and [webpki-roots](https://github.com/rustls/webpki-roots) for certificates.
//!
//! These features are mutually exclusive.
//!

mod client;
mod config;
mod connection;
mod error;
pub mod providers;
mod token_provider;
mod util;

pub use client::PubsubClient;
pub use config::ClientConfig;
pub use error::Error;
pub use token_provider::TokenProvider;

pub use twitch_api2::pubsub::{
    automod_queue, channel_bits, channel_bits_badge, channel_points, channel_subscriptions,
    moderation, user_moderation_notifications, Response as ServerMessage, Topic as TopicDef,
    TopicData, Topics as Topic, TwitchResponse,
};
#[cfg(feature = "unsupported")]
#[cfg_attr(nightly, doc(cfg(feature = "unsupported")))]
pub use twitch_api2::pubsub::{
    channel_cheer, channel_sub_gifts, community_points, following, hypetrain, raid, video_playback,
};
pub(crate) use twitch_api2::pubsub::{listen_command, unlisten_command};

#[cfg(all(feature = "native-tls", feature = "rustls-webpki-roots"))]
compile_error!("`native-tls` and `rustls-webpki-roots` feature flags are mutually exclusive, enable only one of them");

#[cfg(not(any(feature = "native-tls", feature = "rustls-webpki-roots")))]
compile_error!("One feature flag of `native-tls` and `rustls-webpki-roots` has to be enabled");

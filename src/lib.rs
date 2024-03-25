#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
//! Connect to Twitch `PubSub` from Rust land.
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

mod manager;
pub mod providers;
mod pubsub;
mod token_provider;
mod util;

pub use token_provider::TokenProvider;

pub use twitch_api::pubsub::{
    automod_queue, channel_bits, channel_bits_badge, channel_points, channel_subscriptions,
    moderation, user_moderation_notifications, Topic as TopicDef, TopicData, Topics as Topic,
    TwitchResponse,
};

pub use manager::{create_manager, Sender};
pub use pubsub::PubSubEvent;

#[cfg(feature = "unsupported")]
#[cfg_attr(nightly, doc(cfg(feature = "unsupported")))]
pub use twitch_api::pubsub::{
    channel_cheer, channel_sub_gifts, community_points, following, hypetrain, raid, video_playback,
};

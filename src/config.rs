use crate::token_provider::TokenProvider;
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

/// A configuration for the client.
#[derive(Debug)]
pub struct ClientConfig<T: TokenProvider> {
    /// The provider for new tokens
    pub token_provider: T,

    /// The maximum number of topics for any connection
    ///
    /// Default: 50 (limit by twitch)
    pub max_topics_per_connection: usize,

    /// Limits the creation of new connection to Twitch.
    ///
    /// Default: maximum of one creation at any time.
    pub connection_rate_limiter: Arc<Semaphore>,

    /// Defines the delay until the permit for a new connection is dropped (see `connection_rate_limiter`).
    ///
    /// Default: 2s
    pub new_connection_every: Duration,

    /// Time until the a connection attempt should be aborted.
    ///
    /// Default: 20s
    pub connect_timeout: Duration,
}

impl<T: TokenProvider> ClientConfig<T> {
    /// Creates a new config with the token-provider
    pub fn new(token_provider: T) -> ClientConfig<T> {
        ClientConfig {
            token_provider,
            max_topics_per_connection: 50,

            connection_rate_limiter: Arc::new(Semaphore::new(1)),
            new_connection_every: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(20),
        }
    }
}

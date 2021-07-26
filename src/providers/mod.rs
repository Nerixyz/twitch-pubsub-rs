//! Pre-made token providers.

mod no_token;
mod r#static;

#[cfg(feature = "no-token-provider")]
pub use no_token::NoTokenProvider;
#[cfg(feature = "static-token-provider")]
pub use r#static::StaticTokenProvider;

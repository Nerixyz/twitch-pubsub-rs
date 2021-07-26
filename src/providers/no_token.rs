use crate::{TokenProvider, Topic};
use async_trait::async_trait;

/// This [`TokenProvider`][t] will never provide a token.
///
/// [t]: crate::TokenProvider
#[derive(Debug)]
pub struct NoTokenProvider;

#[async_trait]
impl TokenProvider for NoTokenProvider {
    // doesn't matter since we don't return an error
    type Error = std::io::Error;

    async fn provide_token(&self, _: &Topic) -> Result<Option<String>, Self::Error> {
        Ok(None)
    }

    async fn provide_many(
        &self,
        topics: Vec<Topic>,
    ) -> Result<Vec<(Vec<Topic>, Option<String>)>, Self::Error> {
        Ok(vec![(topics, None)])
    }
}

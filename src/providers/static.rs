use crate::{TokenProvider, Topic};
use async_trait::async_trait;
use std::borrow::Cow;

/// The `StaticTokenProvider` always provides the same static token.
#[derive(Debug)]
pub struct StaticTokenProvider(Cow<'static, str>);

impl StaticTokenProvider {
    /// Create a new `StaticTokenProvider`.
    pub fn new<T: Into<Cow<'static, str>>>(token: T) -> Self {
        Self(token.into())
    }

    fn provide(&self) -> String {
        self.0.to_string()
    }
}

#[async_trait]
impl TokenProvider for StaticTokenProvider {
    // this doesn't really matter - we never return an error
    type Error = std::io::Error;

    async fn provide_token(&self, _: &Topic) -> Result<Option<String>, Self::Error> {
        Ok(Some(self.provide()))
    }

    async fn provide_many(
        &self,
        topics: Vec<Topic>,
    ) -> Result<Vec<(Vec<Topic>, Option<String>)>, Self::Error> {
        Ok(vec![(topics, Some(self.provide()))])
    }
}

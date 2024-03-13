use crate::Topic;
use std::fmt::Debug;
use std::future::Future;

/// This trait defines a token provider.
/// It's used to provide tokens for one or many topics.
pub trait TokenProvider: Send + Debug + Sync + 'static {
    /// An error when providing a token.
    type Error: Send + Sync + std::error::Error;

    /// This function should provide an appropriate token for the `topic`.
    ///
    /// It may not provide a token at all by returning [`None`]
    fn provide_token(
        &self,
        topic: &Topic,
    ) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;

    /// This function provides tokens in chunks
    /// A token can be valid for multiple topics.
    /// How this is done, depends on the application.
    ///
    /// By default this function will just call `provide_token` on each topic.
    ///
    /// But if your application _only_ ever gives out one token, then you can optimize this by providing your own implementation.
    /// You could also group the topics by user-/channel-id and get the token for this id.
    fn provide_many(
        &self,
        topics: Vec<Topic>,
    ) -> impl Future<Output = Result<Vec<(Vec<Topic>, Option<String>)>, Self::Error>> + Send {
        async {
            let mut chunks = Vec::with_capacity(topics.len());
            for topic in topics {
                let token = self.provide_token(&topic).await?;
                chunks.push((vec![topic], token));
            }
            Ok(chunks)
        }
    }
}

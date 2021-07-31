use crate::{Topic, TopicData};

/// This is a message from twitch.
pub enum ServerMessage {
    /// The main message
    Data(TopicData),
    /// An error occurred when listening to one or many topic(s).
    /// This is mainly useful when using `listen_many`.
    ListenError(ListenError),
}

/// An error occurred when listening to one or many topic(s).
pub struct ListenError {
    /// The topics requested
    pub topics: Vec<Topic>,
    /// The error code returned from twitch
    pub error: String,
}

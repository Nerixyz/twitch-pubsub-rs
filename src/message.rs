use crate::{Error, TokenProvider, Topic, TopicData};

/// This is a message from twitch.
#[derive(Debug)]
pub enum ServerMessage<T: TokenProvider> {
    /// The main message
    Data(TopicData),
    /// An error occurred when listening to one or many topic(s).
    /// This is mainly useful when using `listen_many`.
    ListenError(ListenError),
    /// A message was received but it could not be parsed
    ParseError(ParseError),
    /// A connection was closed
    ConnectionClosed(ConnectionClosed<T>),
}

/// An error occurred when listening to one or many topic(s).
#[derive(Debug)]
pub struct ListenError {
    /// The topics requested
    pub topics: Vec<Topic>,
    /// The error code returned from twitch
    pub error: String,
}

/// A message was received but couldn't be parsed
#[derive(Debug)]
pub struct ParseError {
    /// The raw message
    pub raw: String,
    /// The error returned by serde_json
    pub error: serde_json::Error,
}

/// A connection was closed
#[derive(Debug)]
pub struct ConnectionClosed<T: TokenProvider> {
    /// The connection id of the connection.
    /// This isn't really relevant but good for debugging.
    pub connection_id: usize,
    /// The reason why it was closed
    pub cause: Error<T>,
}

// reminder: re-export the structs here

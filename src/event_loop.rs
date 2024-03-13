#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLoopAction {
    Continue,
    Stop,
}

impl<E> From<Result<(), E>> for EventLoopAction {
    fn from(value: Result<(), E>) -> Self {
        match value {
            Ok(_) => Self::Continue,
            Err(_) => Self::Stop,
        }
    }
}

use tokio::sync::mpsc;

use super::WsContext;
use crate::{handler::WorkerCommand, EventLoopAction};

pub trait WorkerHandler<M>: Send {
    /// Worker -> Handler
    type Event: Send;
    /// Handler -> Worker
    type Command: Send;

    fn on_message(
        &mut self,
        ctx: &mut WsContext<Self::Event>,
        message: M,
    ) -> impl std::future::Future<Output = EventLoopAction> + Send;
    fn on_command(
        &mut self,
        ctx: &mut WsContext<Self::Event>,
        command: Self::Command,
    ) -> impl std::future::Future<Output = EventLoopAction> + Send;

    fn on_created(
        &mut self,
        _ctx: &mut WsContext<Self::Event>,
        _loopback: Loopback<Self::Command>,
    ) -> impl std::future::Future<Output = ()> + Send {
        async {}
    }
}

pub struct Loopback<C> {
    tx: mpsc::WeakUnboundedSender<WorkerCommand<C>>,
}

impl<C> Loopback<C> {
    pub(crate) fn new(tx: mpsc::WeakUnboundedSender<WorkerCommand<C>>) -> Self {
        Self { tx }
    }

    pub fn try_send(&self, command: C) -> bool {
        let Some(locked) = self.tx.upgrade() else {
            return false;
        };

        locked.send(WorkerCommand::User(command)).is_ok()
    }
}

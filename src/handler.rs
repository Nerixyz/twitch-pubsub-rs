use std::{collections::HashMap, fmt::Display, str::FromStr};

use crate::{
    worker::{self, WorkerHandler},
    EventLoopAction,
};
use tokio::sync::mpsc;
use url::Url;

pub enum WorkerEvent<T> {
    Closed(usize),
    User(usize, T),
}

pub enum WorkerCommand<T> {
    Close,
    User(T),
}

pub struct Worker<H: Handler + ?Sized> {
    id: usize,
    tx: mpsc::UnboundedSender<WorkerCommand<H::Command>>,
    data: H::WorkerData,
}

pub struct HandlerContext<H: Handler + ?Sized> {
    workers: HashMap<usize, Worker<H>>,
    next_id: usize,

    url: Url,

    message_tx: mpsc::Sender<WorkerEvent<H::Message>>,
    message_rx: mpsc::Receiver<WorkerEvent<H::Message>>,

    event_tx: mpsc::UnboundedSender<H::Event>,

    external_loopback_tx: mpsc::WeakUnboundedSender<H::External>,
}

pub trait Handler {
    /// Worker -> Handler
    type Message;
    /// Handler -> Worker
    type Command;
    /// External command
    type External;
    /// Events emitted by the handler
    type Event;

    /// Data for a worker
    type WorkerData;

    fn on_message(
        &mut self,
        worker: &mut Worker<Self>,
        tx: &mpsc::UnboundedSender<Self::Event>,
        message: Self::Message,
    ) -> impl std::future::Future<Output = EventLoopAction> + std::marker::Send;
    fn on_worker_closed(
        &mut self,
        ctx: &mut HandlerContext<Self>,
        id: usize,
        data: Self::WorkerData,
    ) -> impl std::future::Future<Output = EventLoopAction> + std::marker::Send;

    fn on_external(
        &mut self,
        ctx: &mut HandlerContext<Self>,
        external: Self::External,
    ) -> impl std::future::Future<Output = EventLoopAction> + std::marker::Send;
}

impl<H: Handler> HandlerContext<H> {
    pub fn find_mut(
        &mut self,
        by: impl Fn(usize, &H::WorkerData) -> bool,
    ) -> Option<&mut Worker<H>> {
        for (id, w) in self.workers.iter_mut() {
            if by(*id, &w.data) {
                return Some(w);
            }
        }

        None
    }

    pub fn create<W, M>(&mut self, data: H::WorkerData, worker: W)
    where
        W: WorkerHandler<M, Event = H::Message, Command = H::Command> + 'static,
        W::Event: 'static,
        W::Command: 'static,
        M: FromStr + Send,
        M::Err: Display + Send,
    {
        self.create_with(move |_| (data, worker))
    }

    pub fn create_with<W, M>(&mut self, make: impl FnOnce(usize) -> (H::WorkerData, W))
    where
        W: WorkerHandler<M, Event = H::Message, Command = H::Command> + 'static,
        W::Event: 'static,
        W::Command: 'static,
        M: FromStr + Send,
        M::Err: Display + Send,
    {
        let id = self.next_id;
        self.next_id += 1;
        let (data, worker) = make(id);
        let tx = worker::make_connection(id, self.message_tx.clone(), self.url.clone(), worker);

        self.workers.insert(id, Worker { id, tx, data });
    }

    pub fn loopback(&self) -> mpsc::WeakUnboundedSender<H::External> {
        self.external_loopback_tx.clone()
    }

    pub fn emit(&self, event: H::Event) -> Result<(), mpsc::error::SendError<H::Event>> {
        self.event_tx.send(event)
    }
}

impl<H: Handler> Worker<H> {
    pub fn id(&self) -> usize {
        self.id
    }

    pub fn data(&self) -> &H::WorkerData {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut H::WorkerData {
        &mut self.data
    }

    pub fn close(&self) -> Result<(), mpsc::error::SendError<WorkerCommand<H::Command>>> {
        self.tx.send(WorkerCommand::Close)
    }

    pub fn send(
        &self,
        command: H::Command,
    ) -> Result<(), mpsc::error::SendError<WorkerCommand<H::Command>>> {
        self.tx.send(WorkerCommand::User(command))
    }
}

pub async fn run_handler<H>(
    mut handler: H,
    mut ctx: HandlerContext<H>,
    mut rx: mpsc::UnboundedReceiver<H::External>,
) where
    H: Handler,
{
    loop {
        let action = tokio::select! {
            Some(worker) = ctx.message_rx.recv() => {
                match worker {
                    WorkerEvent::Closed(id) => {
                        let Some(worker) = ctx.workers.remove(&id) else {
                            continue;
                        };
                        handler.on_worker_closed(&mut ctx, id, worker.data).await
                    },
                    WorkerEvent::User(id, msg) => {
                        match ctx.workers.get_mut(&id) {
                            Some(worker) => handler.on_message(worker, &ctx.event_tx, msg).await,
                            None => EventLoopAction::Continue,
                        }
                    },
                }
            },
            Some(external) = rx.recv() => {
                handler.on_external(&mut ctx, external).await
            },
            else => EventLoopAction::Stop,
        };
        match action {
            EventLoopAction::Continue => (),
            EventLoopAction::Stop => break,
        }
    }
}

pub fn create_handler<H>(
    url: Url,
    event_tx: mpsc::UnboundedSender<H::Event>,
    external_loopback_tx: mpsc::WeakUnboundedSender<H::External>,
) -> HandlerContext<H>
where
    H: Handler,
{
    let (message_tx, message_rx) = mpsc::channel(16);

    HandlerContext {
        workers: HashMap::new(),
        next_id: 0,
        url,
        message_tx,
        message_rx,
        event_tx,
        external_loopback_tx,
    }
}

pub fn spawn_handler<H>(
    handler: H,
    ctx: HandlerContext<H>,
    rx: mpsc::UnboundedReceiver<H::External>,
) where
    H: Handler + Send + 'static,
    H::External: Send,
    H::Event: Send,
    H::WorkerData: Send,
    H::Message: Send,
    H::Command: Send,
{
    tokio::spawn(async move {
        run_handler(handler, ctx, rx).await;
    });
}

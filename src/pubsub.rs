use std::{collections::HashMap, str::FromStr, time::Duration};

use tokio::sync::mpsc;
use tracing::warn;
use twitch_api::pubsub::{self, Topics};

use crate::{util::generate_nonce, TokenProvider};
use ws_pool::{
    EventLoopAction, Handler, HandlerContext, Loopback, Worker, WorkerHandler, WsContext,
};

const TOPICS_PER_CONNECTION: usize = 50;

struct PubSubWorker {}

pub enum WorkerEvent {
    Connected,
    Message(pubsub::TopicData),
    Response(pubsub::TwitchResponse),
}

pub enum WorkerCommand {
    Subscribe {
        topics: Vec<Topics>,
        auth_token: Option<String>,
        nonce: String,
    },
    Ping,
}

struct PubSubMessage(pubsub::Response);

impl WorkerHandler<PubSubMessage> for PubSubWorker {
    type Event = WorkerEvent;

    type Command = WorkerCommand;

    async fn on_message(
        &mut self,
        ctx: &mut WsContext<Self::Event>,
        PubSubMessage(message): PubSubMessage,
    ) -> EventLoopAction {
        match message {
            pubsub::Response::Response(res) => ctx.emit(WorkerEvent::Response(res)).await.into(),
            pubsub::Response::Message { data } => ctx.emit(WorkerEvent::Message(data)).await.into(),
            pubsub::Response::Pong => EventLoopAction::Continue,
            pubsub::Response::Reconnect => {
                // we can't reuse the topics
                EventLoopAction::Stop
            }
            _ => {
                warn!("Unhandled response");
                EventLoopAction::Continue
            }
        }
    }

    async fn on_command(
        &mut self,
        ctx: &mut WsContext<Self::Event>,
        command: Self::Command,
    ) -> EventLoopAction {
        match command {
            WorkerCommand::Subscribe {
                topics,
                ref auth_token,
                nonce,
            } => {
                let msg = match pubsub::listen_command(
                    &topics,
                    auth_token.as_ref().map(|s| s.as_str()),
                    nonce.as_ref(),
                ) {
                    Ok(m) => m,
                    Err(e) => {
                        warn!(%e, "Failed to serialize command");
                        return EventLoopAction::Continue;
                    }
                };
                ctx.send(&msg).await.into()
            }
            WorkerCommand::Ping => ctx.send("{\"type\":\"PING\"}").await.into(),
        }
    }

    async fn on_created(
        &mut self,
        ctx: &mut WsContext<Self::Event>,
        loopback: Loopback<Self::Command>,
    ) {
        ctx.set_timeout(Duration::from_secs(90));
        let _ = ctx.emit(WorkerEvent::Connected).await;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                if !loopback.try_send(Self::Command::Ping) {
                    break;
                }
            }
        });
    }
}

impl FromStr for PubSubMessage {
    type Err = twitch_api::DeserError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        pubsub::Response::parse(s).map(Self)
    }
}

pub struct PubSubHandler<T> {
    provider: T,
}

struct PendingSub {
    topics: Vec<Topics>,
    auth: Option<String>,
}

pub struct WorkerData {
    topics: Vec<Topics>,
    unconfirmed: HashMap<String, Vec<Topics>>,
    pending_subscriptions: Vec<PendingSub>,
    total_subs: usize,
    connected: bool,
}

pub enum PubSubCommand {
    Subscribe(Vec<Topics>),
}

/// An event from the eventloop
pub enum PubSubEvent<T: TokenProvider> {
    /// A PubSub message was received
    Message(pubsub::TopicData),
    /// A subscription to some topics didn't work
    SubError {
        /// The affected topics
        topics: Vec<Topics>,
        /// The error received from Twitch
        error: String,
    },
    /// The [TokenProvider] failed to provide a token
    ProvideError(T::Error),
}

impl<T> Handler for PubSubHandler<T>
where
    T: TokenProvider,
{
    type Message = WorkerEvent;

    type Command = WorkerCommand;

    type External = PubSubCommand;

    type Event = PubSubEvent<T>;

    type WorkerData = WorkerData;

    async fn on_message(
        &mut self,
        worker: &mut Worker<Self>,
        tx: &mpsc::UnboundedSender<Self::Event>,
        message: Self::Message,
    ) -> EventLoopAction {
        match message {
            WorkerEvent::Connected => {
                debug_assert!(!worker.data().connected);

                worker.data_mut().connected = true;
                let pending = std::mem::take(&mut worker.data_mut().pending_subscriptions);
                for sub in pending {
                    let nonce = generate_nonce(rand::thread_rng());
                    let _ = worker.send(WorkerCommand::Subscribe {
                        topics: sub.topics.clone(),
                        auth_token: sub.auth,
                        nonce: nonce.clone(),
                    });
                    worker.data_mut().unconfirmed.insert(nonce, sub.topics);
                }
                EventLoopAction::Continue
            }
            WorkerEvent::Message(m) => tx.send(PubSubEvent::Message(m)).into(),
            WorkerEvent::Response(res) => {
                let Some(nonce) = res.nonce else {
                    warn!("Received response without nonce");
                    return EventLoopAction::Continue;
                };
                let Some(topics) = worker.data_mut().unconfirmed.remove(&nonce) else {
                    warn!("Received response for unknown nonce");
                    return EventLoopAction::Continue;
                };

                if let Some(error) = res.error.filter(|e| !e.is_empty()) {
                    worker.data_mut().total_subs -= topics.len();
                    tx.send(PubSubEvent::SubError { topics, error }).into()
                } else {
                    worker.data_mut().topics.extend_from_slice(&topics);
                    EventLoopAction::Continue
                }
            }
        }
    }

    async fn on_worker_closed(
        &mut self,
        ctx: &mut HandlerContext<Self>,
        _id: usize,
        data: Self::WorkerData,
    ) -> EventLoopAction {
        let mut all = data.topics;
        all.extend(data.unconfirmed.into_values().flatten());
        all.extend(
            data.pending_subscriptions
                .into_iter()
                .flat_map(|s| s.topics),
        );
        let Some(loopback) = ctx.loopback().upgrade() else {
            return EventLoopAction::Stop;
        };
        loopback.send(PubSubCommand::Subscribe(all)).into()
    }

    async fn on_external(
        &mut self,
        ctx: &mut HandlerContext<Self>,
        external: Self::External,
    ) -> EventLoopAction {
        match external {
            PubSubCommand::Subscribe(topics) => {
                let subs = self.provider.provide_many(topics).await;
                match subs {
                    Ok(subs) => {
                        self.subscribe_to(ctx, subs);
                        EventLoopAction::Continue
                    }
                    Err(e) => ctx.emit(PubSubEvent::ProvideError(e)).into(),
                }
            }
        }
    }
}

impl<T: TokenProvider> PubSubHandler<T> {
    pub fn new(provider: T) -> Self {
        Self { provider }
    }

    fn subscribe_to(
        &mut self,
        ctx: &mut HandlerContext<Self>,
        subs: Vec<(Vec<Topics>, Option<String>)>,
    ) {
        for (mut topics, token) in subs {
            while !topics.is_empty() {
                // Take at most [TOPICS_PER_CONNECTION] subs
                let to_sub = if topics.len() <= TOPICS_PER_CONNECTION {
                    std::mem::take(&mut topics)
                } else {
                    topics.split_off(TOPICS_PER_CONNECTION)
                };

                match ctx
                    .find_mut(|_, data| data.total_subs + to_sub.len() <= TOPICS_PER_CONNECTION)
                {
                    // Fits in existing connection
                    Some(ws) => {
                        if ws.data_mut().connected {
                            let nonce = generate_nonce(rand::thread_rng());
                            ws.data_mut()
                                .unconfirmed
                                .insert(nonce.clone(), to_sub.clone());
                            ws.data_mut().total_subs += to_sub.len();
                            let _ = ws.send(WorkerCommand::Subscribe {
                                topics: to_sub,
                                auth_token: token.clone(),
                                nonce,
                            });
                        } else {
                            ws.data_mut().pending_subscriptions.push(PendingSub {
                                topics: to_sub,
                                auth: token.clone(),
                            });
                        }
                    }
                    // Create new connection with batch
                    None => {
                        let total_subs = to_sub.len();
                        ctx.create(
                            WorkerData {
                                connected: false,
                                pending_subscriptions: vec![PendingSub {
                                    topics: to_sub,
                                    auth: token.clone(),
                                }],
                                topics: vec![],
                                unconfirmed: HashMap::new(),
                                total_subs,
                            },
                            PubSubWorker {},
                        );
                    }
                }
            }
        }
    }
}

use std::fmt::Display;
use std::str::FromStr;

use tokio::sync::mpsc;
use tracing::{debug, info_span, warn};
use url::Url;

use crate::handler::{WorkerCommand, WorkerEvent};
use crate::{ws, EventLoopAction};

mod context;
mod handler;

pub use context::WsContext;
pub use handler::{Loopback, WorkerHandler};

pub fn make_connection<W, M>(
    id: usize,
    events_tx: mpsc::Sender<WorkerEvent<W::Event>>,
    url: Url,
    mut handler: W,
) -> mpsc::UnboundedSender<WorkerCommand<W::Command>>
where
    W: WorkerHandler<M> + 'static,
    W::Event: 'static,
    W::Command: 'static,
    M: FromStr + Send,
    M::Err: Display + Send,
{
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    let loopback = command_tx.downgrade();
    tokio::spawn(async move {
        match ws::connect(&url).await {
            Ok(sock) => {
                let mut ctx = WsContext::new(id, sock, events_tx);
                handler.on_created(&mut ctx, Loopback::new(loopback)).await;
                connection_task(ctx, command_rx, handler).await
            }
            Err(e) => {
                warn!("Failed to start websocket: {e}");
                let res = events_tx.send(WorkerEvent::Closed(id)).await;
                if res.is_err() {
                    warn!("Failed to notify manager of failed connection");
                }
            }
        }
    });

    command_tx
}

async fn connection_task<W, M>(
    mut ctx: WsContext<W::Event>,
    mut command_rx: mpsc::UnboundedReceiver<WorkerCommand<W::Command>>,
    mut handler: W,
) where
    W: WorkerHandler<M>,
    M: FromStr,
    M::Err: Display,
{
    let span = info_span!("connection", id = %ctx.id());
    let _guard = span.enter();

    loop {
        let action = tokio::select! {
            msg = ctx.read::<M>() => {
                match msg {
                    Ok(msg) => {
                        handler.on_message(&mut ctx, msg).await
                    },
                    Err(context::WsReadError::Timeout) => {
                        warn!("Read timed out");
                        EventLoopAction::Stop
                    }
                    Err(err) => {
                        warn!(%err, "Failed to read message");
                        EventLoopAction::Continue
                    },
                }
            },
            command = command_rx.recv() => {
                match command {
                    Some(WorkerCommand::User(command)) => {
                        handler.on_command(&mut ctx, command).await
                    },
                    None | Some(WorkerCommand::Close) => {
                        debug!("Received close signal");
                        EventLoopAction::Stop
                    },
                }
            }
        };

        match action {
            EventLoopAction::Continue => (),
            EventLoopAction::Stop => break,
        }
    }

    debug!("Closing");
    ctx.close().await;
}

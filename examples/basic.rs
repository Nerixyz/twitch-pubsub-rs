use twitch_pubsub::{
    create_manager, providers::StaticTokenProvider, video_playback::VideoPlayback, PubSubEvent,
    Topic,
};
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let arg = std::env::args()
        .nth(1)
        .expect("Pass a username as the first argument");

    let (tx, mut rx) = create_manager(
        StaticTokenProvider::new("".to_owned()),
        Url::parse("wss://pubsub-edge.twitch.tv")?,
    );

    assert!(tx.listen(vec![Topic::VideoPlayback(VideoPlayback {
        channel_login: arg.into(),
    })]));

    while let Some(event) = rx.recv().await {
        match event {
            PubSubEvent::Message(msg) => match msg {
                twitch_pubsub::TopicData::VideoPlayback { topic, reply } => {
                    println!("{}: {:?}", topic.channel_login, reply);
                }
                _ => todo!(),
            },
            PubSubEvent::SubError { error, .. } => {
                println!("Failed to subscribe: {error}")
            }
            PubSubEvent::ProvideError(err) => {
                println!("Failed to generate token; {err}");
            }
        }
    }

    Ok(())
}

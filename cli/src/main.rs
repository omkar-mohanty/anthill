use std::str::FromStr;

use async_std::task::spawn;
use clap::Parser;
use futures::{channel::mpsc, StreamExt};
use network::{client::Client, config::NetworkMode, events::Event};
use tokio::io::{self, AsyncBufReadExt};

#[derive(Parser)]
pub struct Cli {
    /// IPFS only for now  
    #[clap(subcommand)]
    command: Option<NetworkMode>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let (client, event_receiver) = network::new().await.expect("Event loop not formed");

    spawn(event_receiver_loop(event_receiver));

    handle_client(client, cli.command).await
}

async fn handle_client(
    mut client: Client,
    mode: Option<NetworkMode>,
) -> Result<(), Box<dyn std::error::Error>> {
    client
        .start_listening("/ip4/0.0.0.0/tcp/0".parse()?)
        .await
        .expect("Should listen on addressed");

    if let Some(mode) = mode {
        match mode {
            NetworkMode::Test { id, addr } => {
                client
                    .dial(id, addr)
                    .await
                    .expect("Receiver not to be dropped");
                client
                    .subscribe(String::from_str("test-topic")?)
                    .await
                    .expect("Receiver not to be dropped");
            }
            _ => {}
        }
    } else {
        client
            .subscribe(String::from_str("test-topic")?)
            .await
            .expect("Receiver not to be dropped");
    }

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    loop {
        tokio::select! {
            line = stdin.next_line() => {
                let line = line?.expect("stdin closed");
                let topic = String::from_str("test-topic")?;
                client.send_message(topic,line).await.expect("client not to be closed");
            }
        }
    }
}

async fn event_receiver_loop(mut event_receiver: mpsc::Receiver<Event>) {
    futures::select! {
        event = event_receiver.select_next_some() => {
            handle_event(event).await;
        }
    }
}

async fn handle_event(event: Event) {
    match event {
        Event::NewListenAddr { addr } => {
            println!("Starting Listening at {}", addr);
        }
        Event::Subscribed { peer_id, topic } => {
            println!("Peer {} joined {}", peer_id, topic)
        }
        Event::Unsubscribed { peer_id, topic } => {
            println!("Peer {} left {}", peer_id, topic)
        }
        Event::Message {
            propagation_source,
            message,
            ..
        } => {
            println!(
                "{}: {}",
                propagation_source,
                String::from_utf8_lossy(&message.data)
            )
        }
        _ => {}
    }
}

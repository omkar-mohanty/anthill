use async_std::task::spawn;
use clap::Parser;
use futures::{channel::mpsc, StreamExt};
use network::{config::NetworkMode, events::Event, client::Client};

#[derive(Parser)]
pub struct Cli {
    /// Set IPFS or private network.
    #[clap(subcommand)]
    command: NetworkMode,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let (client, event_receiver) = network::new().await?;

    spawn(event_receiver_loop(event_receiver));

    Ok(())
}

async fn handle_client(client: Client,mode: NetworkMode) -> Result<(), Box<dyn std::error::Error>> {
    match mode {
        NetworkMode::Test { addr, topic } => {
           for addr in addr {
                client.dial(peer_id, addr);
           } 
        }
         _ => {}
    }

    Ok(())
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

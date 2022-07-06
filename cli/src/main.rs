use async_std::{io, task::spawn};
use clap::{Parser, Subcommand};
use futures::{
    channel::{mpsc, oneshot},
    AsyncBufReadExt, SinkExt, StreamExt,
};
use libp2p::{gossipsub::IdentTopic, Multiaddr, PeerId};
use network::{commands::Command, eventloop::EventLoop, events::Event};

#[derive(Parser, Debug)]
struct Args {
    #[clap(subcommand)]
    command: Option<CliCommands>,
}

#[derive(Subcommand, Debug)]
enum CliCommands {
    //Dials the Peer
    Dial {
        #[clap(short, long, action, value_name = "id")]
        id: PeerId,
        #[clap(short, long, action, value_name = "addr")]
        addr: Multiaddr,
    },
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let (event_sender, mut event_receiver) = mpsc::channel(1);

    let (mut command_sender, command_receiver) = mpsc::channel(1);

    let eventloop = EventLoop::new(event_sender, command_receiver).await?;

    spawn(eventloop.run());

    let (tx, rx) = oneshot::channel();
    command_sender
        .send(Command::StartListening {
            addr: "/ip4/0.0.0.0/tcp/0".parse()?,
            sender: tx,
        })
        .await?;
    let _ = rx.await?;

    if let Some(CliCommands::Dial { id, addr }) = args.command {
        println!("Dialing peer {:?}", id);
        let (tx, rx) = oneshot::channel();
        command_sender
            .send(Command::Dial {
                peer_id: id,
                peer_addr: addr,
                sender: tx,
            })
            .await?;
        let _ = rx.await;
    }

    let topic = IdentTopic::new("test-topic");

    let (tx, rx) = oneshot::channel();

    command_sender
        .send(network::commands::Command::Subscribe {
            topic: topic.clone(),
            sender: tx,
        })
        .await?;
    let _ = rx.await;
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();
    loop {
        futures::select! {
            lines = stdin.select_next_some() => {
                if "/exit" == lines.as_ref().expect("Stdin not to be closed").clone().as_str() {
                    std::process::exit(0);
                }
                let (tx, rx) = oneshot::channel();
                command_sender.send(network::commands::Command::SendMessage { topic: topic.clone(), message: lines.expect("Stdin not to close"), sender: tx }).await?;
                let _ = rx.await?;
            }
          event = event_receiver.select_next_some() => {
                handle_event(event);
          }
        }
    }
}

fn handle_event(event: Event) {
    match event {
        Event::NewListenAddr { addr } => {
            println!("Listening on  {:?}", addr);
        }
        Event::ConnectionEstablished { peer_id, .. } => {
            println!("Connection established to  {:?}", peer_id);
        }
        Event::Message {
            propagation_source,
            message,
            ..
        } => {
            println!("Here");
            println!(
                "{}: {}",
                propagation_source,
                String::from_utf8_lossy(&message.data)
            );
        }
        Event::Subscribed { peer_id, topic } => {
            println!("{}, subscribed to {}", peer_id, topic.as_str());
        }

        _ => {}
    }
}

use std::str::FromStr;

use async_std::{
    io::{self, prelude::BufReadExt},
    task::spawn,
};
use futures::{channel::oneshot, select, SinkExt, StreamExt};
use network::{commands::Command, eventloop};
use libp2p::PeerId;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (eventloop, mut command_sender, _event_receiver, peer_id) =
        eventloop::EventLoop::default().await?;
    spawn(eventloop.run());
    let topic = libp2p::gossipsub::IdentTopic::new("chat");
    let (tx, rc) = oneshot::channel();
    command_sender
        .send(Command::StartListening {
            addr: "/ip4/0.0.0.0/tcp/0".parse()?,
            sender: tx,
        })
        .await?;
    let _ = rc.await?;
    let (tx, rc) = oneshot::channel();
    command_sender
        .send(Command::Subscribe {
            topic: topic.clone(),
            sender: tx,
        })
        .await?;
    let _ = rc.await?;

    println!("{}", peer_id);

    if let Some(peer_id) = std::env::args().nth(2) {
        let (tx, rc) = oneshot::channel();
        command_sender
            .send(Command::Dial {
                peer_id:PeerId::from_str(peer_id.as_str())?,
                peer_addr: format!("/p2p/{}",peer_id).parse()?,
                sender: tx,
            })
            .await?;
        let _ = rc.await?;
    }

    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();
    loop {
        select! {
            lines = stdin.select_next_some() => {

                let (tx, _) = oneshot::channel();
                command_sender.send(Command::SendMessage { topic: topic.clone() , message: lines?, sender: tx }).await?;

            }
        }
    }
}

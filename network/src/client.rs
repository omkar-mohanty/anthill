use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};

use libp2p::{Multiaddr, PeerId};

use std::error::Error;

use super::commands::Command;

/// Imoteph client interacts with the network eventloop and sends comands and receieves Events.
pub struct Client {
    pub command_sender: mpsc::Sender<Command>,
}

impl Client {
    /// Listen for incoming connections on the given address.
    pub async fn start_listening(&mut self, addr: Multiaddr) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();

        self.command_sender
            .send(Command::StartListening { addr, sender })
            .await
            .expect("Command Receiver not to be dropped");

        receiver.await.expect("sender not to be dropped")
    }

    /// Dial the given peer at the given address
    pub async fn dial(
        &mut self,
        peer_id: PeerId,
        peer_addr: Multiaddr,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();

        self.command_sender
            .send(Command::Dial {
                peer_id,
                peer_addr,
                sender,
            })
            .await
            .expect("Command Receiver not to be dropped");

        receiver.await.expect("Sender not be dropped")
    }

    /// Publish message to the network
    pub async fn send_message(
        &mut self,
        topic: String,
        message: String,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();

        self.command_sender
            .send(Command::SendMessage {
                topic,
                message,
                sender,
            })
            .await
            .expect("Command Receiver not to be dropped");
        receiver.await.expect("Sender not to be dropped")
    }

    /// Subscribe to a gossipsub topic
    pub async fn subscribe(&mut self, topic: String) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();

        self.command_sender
            .send(Command::Subscribe { topic, sender })
            .await
            .expect("Command receiver not to be dropped");

        receiver.await.expect("Sender not to be dropped")
    }
}

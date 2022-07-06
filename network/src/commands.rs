use futures::channel::oneshot::Sender;
use libp2p::{gossipsub::IdentTopic as Topic, Multiaddr, PeerId};
use std::error::Error;
#[derive(Debug)]
pub enum Command {
    StartListening {
        addr: Multiaddr,
        sender: Sender<Result<(), Box<dyn Error + Send>>>,
    },
    Dial {
        peer_id: PeerId,
        peer_addr: Multiaddr,
        sender: Sender<Result<(), Box<dyn Error + Send>>>,
    },
    SendMessage {
        topic: Topic,
        message: String,
        sender: Sender<Result<(), Box<dyn Error + Send>>>,
    },
    Subscribe {
        topic: Topic,
        sender: Sender<Result<(), Box<dyn Error + Send>>>,
    },
    PeerId {
        sender: Sender<PeerId>,
    },
    ListeningAddr {
        sender: Sender<Multiaddr>,
    },
}

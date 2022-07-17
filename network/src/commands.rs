use futures::channel::oneshot::Sender;
use libp2p::{Multiaddr, PeerId};
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
        topic: String,
        message: String,
        sender: Sender<Result<(), Box<dyn Error + Send>>>,
    },
    Subscribe {
        topic: String,
        sender: Sender<Result<(), Box<dyn Error + Send>>>,
    },
    PeerId {
        sender: Sender<PeerId>,
    },
    ListeningAddr {
        sender: Sender<Multiaddr>,
    },
    IpfsInit {
        sender: Sender<Result<(), Box<dyn Error + Send>>>,
    },
    IpfsDial {
        peer_id: PeerId,
        sender: Sender<Result<(), Box<dyn Error + Send>>>,
    },
}

use super::eventloop::FileResponse;
use futures::channel::oneshot::Sender;
use libp2p::{
    gossipsub::IdentTopic as Topic, request_response::ResponseChannel, Multiaddr, PeerId,
};
use std::{collections::HashSet, error::Error};
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
    StartProviding {
        file_name: String,
        sender: Sender<()>,
    },
    GetProviders {
        file_name: String,
        sender: Sender<HashSet<PeerId>>,
    },
    RequestFile {
        file_name: String,
        peer: PeerId,
        sender: Sender<Result<String, Box<dyn Error + Send>>>,
    },
    RespondFile {
        file: String,
        channel: ResponseChannel<FileResponse>,
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
}

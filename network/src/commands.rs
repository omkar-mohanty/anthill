use super::eventloop::FileResponse;
use libp2p::{request_response::ResponseChannel, Multiaddr, PeerId};
use std::{collections::HashSet, error::Error};
use futures::channel::oneshot::Sender;
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
}

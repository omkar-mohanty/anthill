use libp2p::PeerId;

pub enum Event {
    ConnectionEstablished(PeerId),
    ConnectionClosed(PeerId),
}

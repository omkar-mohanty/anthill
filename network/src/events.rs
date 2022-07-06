use libp2p::{
    core::ConnectedPoint,
    gossipsub::{GossipsubMessage, MessageId, TopicHash},
    Multiaddr, PeerId,
};

pub enum Event {
    IncomingConnection {
        local_addr: Multiaddr,
        send_back_addr: Multiaddr,
    },
    ConnectionEstablished {
        peer_id: PeerId,
        endpoint: ConnectedPoint,
    },
    ConnectionClosed(PeerId),
    NewListenAddr {
        addr: Multiaddr,
    },
    Subscribed {
        peer_id: PeerId,
        topic: TopicHash,
    },
    Message {
        propagation_source: PeerId,
        message_id: MessageId,
        message: GossipsubMessage,
    },
    Unsubscribed {
        peer_id: PeerId,
        topic: TopicHash,
    },
}

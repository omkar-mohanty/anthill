use super::{commands::Command, events::Event};

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io;
use std::str::FromStr;
use std::time::Duration;

use libp2p::core::either::EitherError;
use libp2p::core::ConnectedPoint;
use libp2p::gossipsub::error::GossipsubHandlerError;
use libp2p::gossipsub::{
    Gossipsub, GossipsubConfigBuilder, GossipsubEvent, GossipsubMessage, IdentTopic as Topic,
    MessageId, TopicHash,
};
use libp2p::kad::store::MemoryStore;
use libp2p::kad::{GetClosestPeersError, Kademlia, KademliaConfig, KademliaEvent, QueryResult};
use libp2p::swarm::SwarmEvent;
use libp2p::{development_transport, identity, Multiaddr};
use libp2p::{multiaddr::Protocol, NetworkBehaviour, PeerId, Swarm};

use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};

/// Main event loop of the application
/// Can run independently from any other component.
pub struct EventLoop {
    /// Libp2p swarm
    swarm: Swarm<ComposedBehaviour>,
    /// All connected peers to the current node
    connected_peers: HashMap<PeerId, ConnectedPoint>,
    /// All messages received by the node
    messages: HashMap<MessageId, GossipsubMessage>,
    /// Network event manager
    event_sender: mpsc::Sender<Event>,
    /// Command receiver from client/user
    command_receiver: mpsc::Receiver<Command>,
    /// Current listening address of the node
    listening_addr: Multiaddr,
    /// Topics the node is sibscribed to
    topics: HashMap<TopicHash, Topic>,
}

impl EventLoop {
    async fn handle_event(
        &mut self,
        event: SwarmEvent<ComposedEvent, EitherError<GossipsubHandlerError, io::Error>>,
    ) {
        match event {
            SwarmEvent::Behaviour(ComposedEvent::Gossipsub(event)) => {
                self.handle_gossipsub_event(event).await
            }
            SwarmEvent::Behaviour(ComposedEvent::Kademila(event)) => {
                self.handle_kademila_event(event).await;
            }
            event => self.handle_swarm_event(event).await,
        }
    }

    async fn handle_kademila_event(&mut self, event: KademliaEvent) {
        match event {
            KademliaEvent::OutboundQueryCompleted {
                result: QueryResult::GetClosestPeers(result),
                ..
            } => match result {
                Ok(ok) => {
                    if !ok.peers.is_empty() {
                        for peer in ok.peers {
                            self.swarm
                                .behaviour_mut()
                                .gossipsub
                                .add_explicit_peer(&peer);
                        }
                    }
                }

                Err(GetClosestPeersError::Timeout { peers, .. }) => {
                    if !peers.is_empty() {
                        for peer in peers {
                            self.swarm
                                .behaviour_mut()
                                .gossipsub
                                .add_explicit_peer(&peer);
                        }
                    }
                }
            },
            _ => {}
        }
    }

    async fn handle_gossipsub_event(&mut self, event: GossipsubEvent) {
        match event {
            GossipsubEvent::Message {
                propagation_source,
                message_id,
                message,
            } => {
                self.messages.insert(message_id.clone(), message.clone());

                let _ = self
                    .event_sender
                    .send(Event::Message {
                        propagation_source,
                        message_id,
                        message,
                    })
                    .await;
            }
            GossipsubEvent::Subscribed { peer_id, topic } => {
                let _ = self
                    .event_sender
                    .send(Event::Subscribed { peer_id, topic })
                    .await;
            }
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                let _ = self
                    .event_sender
                    .send(Event::Unsubscribed { peer_id, topic })
                    .await;
            }
            GossipsubEvent::GossipsubNotSupported { .. } => {}
        }
    }

    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<ComposedEvent, EitherError<GossipsubHandlerError, io::Error>>,
    ) {
        match event {
            SwarmEvent::Dialing(peer_id) => {
                println!("Dialing {:?}", peer_id);
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                self.connected_peers.insert(peer_id, endpoint.clone());
                let _ = self
                    .event_sender
                    .send(Event::ConnectionEstablished { peer_id, endpoint })
                    .await;
            }

            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                self.connected_peers.remove(&peer_id);
                let _ = self
                    .event_sender
                    .send(Event::ConnectionClosed(peer_id))
                    .await;
            }

            SwarmEvent::NewListenAddr { address, .. } => {
                let local_peer_id = *self.swarm.local_peer_id();

                self.listening_addr = address.with(Protocol::P2p(local_peer_id.into()));

                let _ = self
                    .event_sender
                    .send(Event::NewListenAddr {
                        addr: self.listening_addr.clone(),
                    })
                    .await;
            }

            SwarmEvent::BannedPeer { peer_id, .. } => {
                println!("{:?} Has been banned", peer_id);
            }

            SwarmEvent::IncomingConnection {
                local_addr,
                send_back_addr,
            } => {
                let _ = self
                    .event_sender
                    .send(Event::IncomingConnection {
                        local_addr,
                        send_back_addr,
                    })
                    .await;
            }
            SwarmEvent::IncomingConnectionError { .. } => {}
            SwarmEvent::OutgoingConnectionError { .. } => {}
            SwarmEvent::ExpiredListenAddr { .. } => {}
            SwarmEvent::ListenerError { .. } | SwarmEvent::ListenerClosed { .. } => {}
            e => panic!("{:?}", e),
        }
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::StartListening { addr, sender } => {
                if let Err(err) = self.swarm.listen_on(addr) {
                    let _ = sender.send(Err(Box::new(err)));
                } else {
                    let _ = sender.send(Ok(()));
                }
            }

            Command::Dial {
                peer_id,
                peer_addr,
                sender,
            } => {
                match self.swarm.dial(peer_addr) {
                    Ok(_) => {
                        let bootaddr = Multiaddr::from_str("/dnsaddr/bootstrap.libp2p.io")
                            .expect("Bootaddr not parsed");

                        self.swarm
                            .behaviour_mut()
                            .kademlia
                            .add_address(&peer_id, bootaddr.clone());

                        self.swarm
                            .behaviour_mut()
                            .gossipsub
                            .add_explicit_peer(&peer_id);
                        let _ = sender.send(Ok(()));
                    }
                    Err(err) => {
                        let _ = sender.send(Err(Box::new(err)));
                    }
                };
            }

            Command::SendMessage {
                topic,
                message,
                sender,
            } => {
                match self.swarm.behaviour_mut().gossipsub.publish(topic, message) {
                    Ok(_) => {
                        let _ = sender.send(Ok(()));
                    }
                    Err(err) => {
                        let _ = sender.send(Err(Box::new(err)));
                    }
                };
            }

            Command::Subscribe { topic, sender } => {
                if let Err(err) = self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    self.topics.insert(topic.hash(), topic);
                    let _ = sender.send(Err(Box::new(err)));
                } else {
                    let _ = sender.send(Ok(()));
                }
            }

            Command::PeerId { sender } => {
                let peer_id = *self.swarm.local_peer_id();

                let _ = sender.send(peer_id);
            }

            Command::ListeningAddr { sender } => {
                let listening_addr = self.listening_addr.clone();

                let _ = sender.send(listening_addr);
            }
        }
    }

    /// Creates new event loop with default settings
    pub async fn new(
        event_sender: mpsc::Sender<Event>,
        command_receiver: mpsc::Receiver<Command>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let keypair = identity::Keypair::generate_ed25519();

        let peer_id = PeerId::from(keypair.public());

        let transport = development_transport(keypair.clone()).await?;

        let message_id_fn = |message: &GossipsubMessage| {
            let mut s = DefaultHasher::new();
            message.data.hash(&mut s);
            MessageId::from(s.finish().to_string())
        };

        let swarm = {
            let gossipsub_config = GossipsubConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(1))
                .validation_mode(libp2p::gossipsub::ValidationMode::Strict)
                .message_id_fn(message_id_fn)
                .build()?;

            let gossipsub = Gossipsub::new(
                libp2p::gossipsub::MessageAuthenticity::Signed(keypair),
                gossipsub_config,
            )?;

            let mut kademlia_config = KademliaConfig::default();

            kademlia_config.set_query_timeout(Duration::from_secs(5 * 60));

            let store = MemoryStore::new(peer_id);

            let kademlia = Kademlia::with_config(peer_id, store, kademlia_config);

            let behaviour = ComposedBehaviour {
                gossipsub,
                kademlia,
            };

            Swarm::new(transport, behaviour, peer_id)
        };

        Ok(Self {
            event_sender,
            command_receiver,
            swarm,
            connected_peers: Default::default(),
            messages: Default::default(),
            topics: Default::default(),
            listening_addr: "/ip4/0.0.0.0/tcp/0".parse()?,
        })
    }

    /// Starts the event loop
    /// Best to run in a separate thread
    pub async fn run(mut self) {
        loop {
            futures::select! {
                event = self.swarm.select_next_some() => {
                        self.handle_event(event).await;
                }

                command = self.command_receiver.select_next_some() => {
                        self.handle_command(command).await;
                }
            }
        }
    }
}

///Network Behaviour of the node
/// TODO Add support for Kademilia and request response
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "ComposedEvent")]
pub struct ComposedBehaviour {
    gossipsub: Gossipsub,
    kademlia: Kademlia<MemoryStore>,
}

/// Libp2p events
#[derive(Debug)]
pub enum ComposedEvent {
    Gossipsub(GossipsubEvent),
    Kademila(KademliaEvent),
}

impl From<GossipsubEvent> for ComposedEvent {
    fn from(event: GossipsubEvent) -> Self {
        ComposedEvent::Gossipsub(event)
    }
}

impl From<KademliaEvent> for ComposedEvent {
    fn from(event: KademliaEvent) -> Self {
        ComposedEvent::Kademila(event)
    }
}

#[cfg(test)]
mod tests {
    use async_std::task::spawn;

    use super::*;

    use std::{error, str::FromStr};

    use futures::channel::{mpsc, oneshot};

    use libp2p::gossipsub::IdentTopic as Topic;

    async fn event_loop(
    ) -> Result<(mpsc::Sender<Command>, mpsc::Receiver<Event>), Box<dyn error::Error>> {
        let (event_sender, mut event_receiver) = mpsc::channel(1);

        let (mut command_sender, rx) = mpsc::channel(1);

        let event_loop = EventLoop::new(event_sender, rx).await?;

        spawn(event_loop.run());

        let (tx, rx) = oneshot::channel();

        command_sender
            .send(Command::StartListening {
                addr: "/ip4/0.0.0.0/tcp/0".parse()?,
                sender: tx,
            })
            .await?;

        let _ = rx.await;

        Ok((command_sender, event_receiver))
    }

    #[tokio::test]
    async fn test_default() -> Result<(), Box<dyn error::Error>> {
        let (event_sender, _) = mpsc::channel(10);

        let (_, rx) = mpsc::channel(10);

        let _ = EventLoop::new(event_sender, rx).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_command() -> Result<(), Box<dyn error::Error>> {
        let (event_sender, mut event_receiver) = mpsc::channel(1);

        let (mut command_sender, rx) = mpsc::channel(1);

        let event_loop = EventLoop::new(event_sender, rx).await?;

        spawn(event_loop.run());

        let (tx, rx) = oneshot::channel();

        command_sender
            .send(Command::StartListening {
                addr: "/ip4/0.0.0.0/tcp/0".parse()?,
                sender: tx,
            })
            .await?;

        let _ = rx.await;

        let result = event_receiver.select_next_some().await;

        if let Event::NewListenAddr { .. } = result {
            return Ok(());
        }

        panic!("Failed");
    }

    #[tokio::test]
    async fn test_dial() -> Result<(), Box<dyn error::Error>> {
        let (mut dialed_command, mut dailed_event_receiver) = event_loop().await?;
        let (mut dialer_command, mut dialer_event_receiver) = event_loop().await?;

        let peer_addr = {
            if let Event::NewListenAddr { addr } = dailed_event_receiver.select_next_some().await {
                addr
            } else {
                panic!("Failed to get Multiaddr");
            }
        };

        let peer_id = {
            let (tx, rx) = oneshot::channel();

            let _ = dialed_command.send(Command::PeerId { sender: tx }).await;

            println!("Here");
            rx.await?
        };

        let result = {
            let (sender, rx) = oneshot::channel();

            let _ = dialer_command
                .send(Command::Dial {
                    peer_id,
                    peer_addr,
                    sender,
                })
                .await;

            let _ = rx.await;

            println!("Here 2");
            dailed_event_receiver.select_next_some().await
        };

        if let Event::ConnectionEstablished { peer_id, .. } = result {
            println!("Established connection with {:?}", peer_id);
            return Ok(());
        };
        panic!("Failed");
    }

    #[tokio::test]
    async fn test_message() -> Result<(), Box<dyn error::Error>> {
        let (mut command_sender_messenger, mut event_receiver) = event_loop().await?;

        let (tx, rx) = oneshot::channel();

        command_sender_messenger
            .send(Command::ListeningAddr { sender: tx })
            .await?;

        let addr = rx.await?;

        let (tx, rx) = oneshot::channel();

        command_sender_messenger
            .send(Command::PeerId { sender: tx })
            .await?;

        let peer_id = rx.await?;

        let (mut command_sender_receiver, _) = event_loop().await?;

        let (tx, rx) = oneshot::channel();

        command_sender_receiver
            .send(Command::Dial {
                peer_id,
                peer_addr: addr,
                sender: tx,
            })
            .await?;

        let _ = rx.await?;

        let topic = Topic::new("test-topic");

        let (tx, rx) = oneshot::channel();

        command_sender_messenger
            .send(Command::Subscribe {
                topic: topic.clone(),
                sender: tx,
            })
            .await?;

        let _ = rx.await?;

        let (tx, rx) = oneshot::channel();

        command_sender_receiver
            .send(Command::Subscribe {
                topic: topic.clone(),
                sender: tx,
            })
            .await?;

        let _ = rx.await?;

        let (tx, rx) = oneshot::channel();

        command_sender_receiver
            .send(Command::SendMessage {
                topic: topic.clone(),
                message: String::from_str("testing")?,
                sender: tx,
            })
            .await?;

        let _ = rx.await?;

        Ok(())
    }
}

use super::{commands::Command, events::Event};

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use libp2p::core::ConnectedPoint;
use libp2p::gossipsub::error::GossipsubHandlerError;
use libp2p::gossipsub::{
    Gossipsub, GossipsubConfigBuilder, GossipsubEvent, GossipsubMessage, MessageId, TopicHash,
};
use libp2p::swarm::SwarmEvent;
use libp2p::{development_transport, identity};
use libp2p::{multiaddr::Protocol, NetworkBehaviour, PeerId, Swarm};

use futures::channel::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};

pub struct EventLoop {
    message_handler: Box<dyn Fn(PeerId, MessageId, GossipsubMessage) + Sync + Send>,
    sub_handler: Box<dyn Fn(PeerId, TopicHash) + Sync + Send>,
    unsub_handler: Box<dyn Fn(PeerId, TopicHash) + Sync + Send>,
    swarm: Swarm<ComposedBehaviour>,
    connected_peers: HashMap<PeerId, ConnectedPoint>,
    messages: HashMap<MessageId, GossipsubMessage>,
    event_sender: mpsc::Sender<Event>,
    command_receiver: mpsc::Receiver<Command>,
}

impl EventLoop {
    async fn handle_event(&mut self, event: SwarmEvent<ComposedEvent, GossipsubHandlerError>) {
        match event {
            SwarmEvent::Behaviour(ComposedEvent::Gossipsub(event)) => {
                self.handle_gossipsub_event(event).await
            }
            event => self.handle_swarm_event(event).await,
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

                (self.message_handler)(propagation_source, message_id, message);
            }
            GossipsubEvent::Subscribed { peer_id, topic } => {
                (self.sub_handler)(peer_id, topic);
            }
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                (self.unsub_handler)(peer_id, topic);
            }
            GossipsubEvent::GossipsubNotSupported { .. } => {}
        }
    }

    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<ComposedEvent, GossipsubHandlerError>,
    ) {
        match event {
            SwarmEvent::Dialing(peer_id) => {
                println!("Dialing {:?}", peer_id);
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                self.connected_peers.insert(peer_id, endpoint);
                let _ = self
                    .event_sender
                    .send(Event::ConnectionEstablished(peer_id))
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

                println!(
                    "Local node listening on {:?}",
                    address.with(Protocol::P2p(local_peer_id.into()))
                );
            }

            SwarmEvent::BannedPeer { peer_id, .. } => {
                println!("{:?} Has been banned", peer_id);
            }

            SwarmEvent::IncomingConnection { .. } => {}
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
                let _ = self.swarm.listen_on(addr);
                let _ = sender.send(Ok(()));
            }

            Command::Dial { peer_id, peer_addr, sender } => {
                let _ = self.swarm.dial(peer_addr);

                let _ = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .add_explicit_peer(&peer_id);

                let _ = sender.send(Ok(()));
            }

            Command::SendMessage { topic, message, sender } => {
                let _ = self.swarm.behaviour_mut().gossipsub.publish(topic,message);

                let _ =sender.send(Ok(()));
            }

            Command::Subscribe { topic, sender } => {
                let _ = self.swarm.behaviour_mut().gossipsub.subscribe(&topic);

                let _ = sender.send(Ok(()));
            }
        }
    }

    pub async fn new(
        event_sender: mpsc::Sender<Event>,
        command_receiver: mpsc::Receiver<Command>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let message_handler = Box::new(
            |peer_id: PeerId, _message_id: MessageId, message: GossipsubMessage| {
                println!("{:?}: {:?}", peer_id, message);
            },
        );

        let sub_handler = Box::new(|peer_id: PeerId, topic: TopicHash| {
            println!("{:?} subscribed to {:?}", peer_id, topic);
        });

        let unsub_handler = Box::new(|peer_id: PeerId, topic: TopicHash| {
            println!("{:?} subscribed to {:?}", peer_id, topic);
        });

        let keypair = identity::Keypair::generate_ed25519();

        let peer_id = PeerId::from(keypair.public());

        let transport = development_transport(keypair.clone()).await?;

        let message_id_fn = |message: &GossipsubMessage| {
            let mut s = DefaultHasher::new();
            message.data.hash(&mut s);
            MessageId::from(s.finish().to_string())
        };

        let gossipsub_config = GossipsubConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(libp2p::gossipsub::ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            .build()?;

        let gossipsub = Gossipsub::new(
            libp2p::gossipsub::MessageAuthenticity::Signed(keypair),
            gossipsub_config,
        )?;

        let behaviour = ComposedBehaviour { gossipsub };

        let swarm = Swarm::new(transport, behaviour, peer_id);

        Ok(Self {
            message_handler,
            sub_handler,
            unsub_handler,
            event_sender,
            command_receiver,
            swarm,
            connected_peers: Default::default(),
            messages: Default::default(),
        })
    }

    async fn run(&mut self) {
        
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

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "ComposedEvent")]
pub struct ComposedBehaviour {
    gossipsub: Gossipsub,
}

#[derive(Debug)]
pub enum ComposedEvent {
    Gossipsub(GossipsubEvent),
}

impl From<GossipsubEvent> for ComposedEvent {
    fn from(event: GossipsubEvent) -> Self {
        ComposedEvent::Gossipsub(event)
    }
}

#[cfg(test)]
mod tests {
    use async_std::task::spawn;

    use super::*;

    use std::error;

    #[tokio::test]
    async fn test_default() -> Result<(), Box<dyn error::Error>> {
        let (event_sender, _) = mpsc::channel(1);

        let (_, rx) =mpsc::channel(1);

        let _ = EventLoop::new(event_sender, rx).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_command() -> Result<(), Box<dyn error::Error>> {
        let (event_sender, event_receiver) = mpsc::channel(1);

        let (mut command_sender, rx) =mpsc::channel(1);

        let mut event_loop = EventLoop::new(event_sender, rx).await?;

        spawn( event_loop.run());

        let (tx, rx) =oneshot::channel();

        command_sender.send(Command::StartListening { addr:"/ip4/0.0.0.0/tcp/0".parse()? , sender: tx }).await;

        rx.await;
        Ok(())
    }
}

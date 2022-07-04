use super::commands::Command;
use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::{AsyncRead, AsyncWrite, AsyncWriteExt, SinkExt, StreamExt};
use libp2p::core::either::EitherError;
use libp2p::gossipsub::error::GossipsubHandlerError;
use libp2p::gossipsub::{
    Gossipsub, GossipsubConfigBuilder, GossipsubEvent, MessageAuthenticity, ValidationMode, GossipsubMessage, MessageId,
};
use libp2p::kad::store::MemoryStore;
use libp2p::kad::{GetProvidersOk, Kademlia, KademliaEvent, QueryId, QueryResult};
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{
    ProtocolSupport, RequestId, RequestResponse, RequestResponseEvent, RequestResponseMessage,
    ResponseChannel,
};
use libp2p::swarm::{ConnectionHandlerUpgrErr, Swarm, SwarmEvent};
use libp2p::{
    core::{
        upgrade::{read_length_prefixed, write_length_prefixed},
        ProtocolName,
    },
    development_transport,
    request_response::RequestResponseCodec,
    NetworkBehaviour, PeerId,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    iter,
    time::Duration,
};
/// Main event loop of the sky damemon.  
/// Provides files to peers, queries other peers for files, and recieves files from other peers.
pub struct EventLoop {
    swarm: Swarm<ComposedBehaviour>,
    command_receiver: mpsc::Receiver<Command>,
    event_sender: mpsc::Sender<Event>,
    pending_dial: HashMap<PeerId, oneshot::Sender<Result<(), Box<dyn Error + Send>>>>,
    pending_start_providing: HashMap<QueryId, oneshot::Sender<()>>,
    pending_get_providers: HashMap<QueryId, oneshot::Sender<HashSet<PeerId>>>,
    pending_request_file:
        HashMap<RequestId, oneshot::Sender<Result<String, Box<dyn Error + Send>>>>,
}

impl EventLoop {
    pub fn new(
        swarm: Swarm<ComposedBehaviour>,
        command_receiver: mpsc::Receiver<Command>,
        event_sender: mpsc::Sender<Event>,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            pending_dial: Default::default(),
            pending_start_providing: Default::default(),
            pending_get_providers: Default::default(),
            pending_request_file: Default::default(),
        }
    }

    pub async fn default(
    ) -> Result<(Self, mpsc::Sender<Command>, mpsc::Receiver<Event>, PeerId), Box<dyn Error>> {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from_public_key(&keypair.public());
        let transport = development_transport(keypair.clone()).await?;

        let message_id_fn =  |message: &GossipsubMessage| {
            let mut s = DefaultHasher::new(); 
            message.data.hash(&mut s);
            MessageId::from(s.finish().to_string()) 
        };

        let config = GossipsubConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            .build()
            .expect("Correct Config");

        let behaviour = ComposedBehaviour {
            gossipsub: Gossipsub::new(MessageAuthenticity::Signed(keypair), config)
                .expect("correct configuration"),
            kademlia: Kademlia::new(peer_id, MemoryStore::new(peer_id)),
            request_response: RequestResponse::new(
                FileExchangeCodec(),
                iter::once((FileExchangeProtocol(), ProtocolSupport::Full)),
                Default::default(),
            ),
        };

        let (command_sender, command_receiver) = mpsc::channel::<Command>(1);
        let (event_sender, event_reveiver) = mpsc::channel::<Event>(1);

        let swarm = Swarm::new(transport, behaviour, peer_id.clone());

        Ok((
            Self::new(swarm, command_receiver, event_sender),
            command_sender,
            event_reveiver,
            peer_id,
        ))
    }
    async fn handle_gossipsub_event(&mut self, event: GossipsubEvent) {
        match event {
            GossipsubEvent::Subscribed { peer_id, .. } => {
                println!("{} has joined", peer_id);
            }
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                println!("{:?} has left {:?}", peer_id, topic.to_string());
            }
            GossipsubEvent::Message {
                propagation_source,
                message,
                ..
            } => {
                println!(
                    "{:?} : {:?}",
                    propagation_source,
                    String::from_utf8_lossy(&message.data)
                );
            }
            GossipsubEvent::GossipsubNotSupported { .. } => {}
        }
    }

    async fn handle_request_response_event(
        &mut self,
        event: RequestResponseEvent<FileRequest, FileResponse, FileResponse>,
    ) {
        match event {
            RequestResponseEvent::Message { message, .. } => match message {
                RequestResponseMessage::Request {
                    request, channel, ..
                } => {
                    self.event_sender
                        .send(Event::InboundRequest {
                            request: request.0,
                            channel,
                        })
                        .await
                        .expect("Event receiver not to be dropped.");
                }
                RequestResponseMessage::Response {
                    request_id,
                    response,
                } => {
                    let _ = self
                        .pending_request_file
                        .remove(&request_id)
                        .expect("Request to still be pending.")
                        .send(Ok(response.0));
                }
            },

            RequestResponseEvent::OutboundFailure {
                request_id, error, ..
            } => {
                let _ = self
                    .pending_request_file
                    .remove(&request_id)
                    .expect("Request to still be pending.")
                    .send(Err(Box::new(error)));
            }
            RequestResponseEvent::ResponseSent { .. } => {}
            RequestResponseEvent::InboundFailure { peer, .. } => {
                println!("Failed to process request from {:?}", peer);
            }
        }
    }

    async fn handle_kademilia_event(&mut self, event: KademliaEvent) {
        match event {
            KademliaEvent::OutboundQueryCompleted {
                id,
                result: QueryResult::StartProviding(_),
                ..
            } => {
                let sender: oneshot::Sender<()> = self
                    .pending_start_providing
                    .remove(&id)
                    .expect("Completed query to be previously pending.");
                let _ = sender.send(());
            }

            KademliaEvent::OutboundQueryCompleted {
                id,
                result: QueryResult::GetProviders(Ok(GetProvidersOk { providers, .. })),
                ..
            } => {
                let _ = self
                    .pending_get_providers
                    .remove(&id)
                    .expect("Completed query to be previously pending.")
                    .send(providers);
            }
            _ => {}
        }
    }
    pub async fn run(mut self) {
        loop {
            futures::select! {
                event = self.swarm.select_next_some() => self.handle_event(event).await  ,
                command = self.command_receiver.next() => match command {
                    Some(c) => self.handle_command(c).await,
                    // Command channel closed, thus shutting down the network event loop.
                    None=>  return,
                },
            }
        }
    }

    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<
            ComposedEvent,
            EitherError<
                EitherError<ConnectionHandlerUpgrErr<io::Error>, io::Error>,
                GossipsubHandlerError,
            >,
        >,
    ) {
        match event {
            SwarmEvent::IncomingConnection { .. } => {}
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                if endpoint.is_dialer() {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        let _ = sender.send(Ok(()));
                    }
                }
            }
            SwarmEvent::ConnectionClosed { .. } => {}
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                if let Some(peer_id) = peer_id {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        let _ = sender.send(Err(Box::new(error)));
                    }
                }
            }
            SwarmEvent::IncomingConnectionError { .. } => {}
            SwarmEvent::Dialing(peer_id) => println!("Dialing {}", peer_id),
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {:?}", address);
            }
            e => panic!("{:?}", e),
        }
    }
    async fn handle_event(
        &mut self,
        event: SwarmEvent<
            ComposedEvent,
            EitherError<
                EitherError<ConnectionHandlerUpgrErr<io::Error>, io::Error>,
                GossipsubHandlerError,
            >,
        >,
    ) {
        match event {
            SwarmEvent::Behaviour(ComposedEvent::Kademlia(event)) => {
                self.handle_kademilia_event(event).await
            }
            SwarmEvent::Behaviour(ComposedEvent::RequestResponse(event)) => {
                self.handle_request_response_event(event).await
            }
            SwarmEvent::Behaviour(ComposedEvent::Gossipsub(event)) => {
                self.handle_gossipsub_event(event).await
            }
            event => self.handle_swarm_event(event).await,
        }
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::StartListening { addr, sender } => {
                let _ = match self.swarm.listen_on(addr) {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(Box::new(e))),
                };
            }
            Command::Dial {
                peer_id,
                peer_addr,
                sender,
            } => {
                if self.pending_dial.contains_key(&peer_id) {
                    todo!("Already dialing peer.");
                } else {
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, peer_addr.clone());
                    match self
                        .swarm
                        .dial(peer_addr.with(Protocol::P2p(peer_id.into())))
                    {
                        Ok(()) => {
                            self.pending_dial.insert(peer_id, sender);
                        }
                        Err(e) => {
                            let _ = sender.send(Err(Box::new(e)));
                        }
                    };
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .add_explicit_peer(&peer_id)
                }
            }
            Command::StartProviding { file_name, sender } => {
                let query_id = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .start_providing(file_name.into_bytes().into())
                    .expect("No store error.");
                self.pending_start_providing.insert(query_id, sender);
            }
            Command::GetProviders { file_name, sender } => {
                let query_id = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .get_providers(file_name.into_bytes().into());
                self.pending_get_providers.insert(query_id, sender);
            }
            Command::RequestFile {
                file_name,
                peer,
                sender,
            } => {
                let request_id = self
                    .swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&peer, FileRequest(file_name));
                self.pending_request_file.insert(request_id, sender);
            }
            Command::RespondFile { file, channel } => {
                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, FileResponse(file))
                    .expect("Connection to peer to be still open.");
            }
            Command::SendMessage {
                message,
                topic,
                sender,
            } => {
                println!("Sending message");
                self.swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(topic, message.into_bytes())
                    .expect("Message sent");
                let _ = sender.send(Ok(()));
            }
            Command::Subscribe { topic, sender } => {
                self.swarm
                    .behaviour_mut()
                    .gossipsub
                    .subscribe(&topic)
                    .expect("Subscript to topic");
                let _ = sender.send(Ok(()));
            }
        }
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "ComposedEvent")]
pub struct ComposedBehaviour {
    request_response: RequestResponse<FileExchangeCodec>,
    kademlia: Kademlia<MemoryStore>,
    gossipsub: Gossipsub,
}

#[derive(Debug)]
pub enum ComposedEvent {
    RequestResponse(RequestResponseEvent<FileRequest, FileResponse>),
    Kademlia(KademliaEvent),
    Gossipsub(GossipsubEvent),
}

impl From<RequestResponseEvent<FileRequest, FileResponse>> for ComposedEvent {
    fn from(event: RequestResponseEvent<FileRequest, FileResponse>) -> Self {
        ComposedEvent::RequestResponse(event)
    }
}

impl From<KademliaEvent> for ComposedEvent {
    fn from(event: KademliaEvent) -> Self {
        ComposedEvent::Kademlia(event)
    }
}

impl From<GossipsubEvent> for ComposedEvent {
    fn from(event: GossipsubEvent) -> Self {
        ComposedEvent::Gossipsub(event)
    }
}
#[derive(Debug)]
pub enum Event {
    InboundRequest {
        request: String,
        channel: ResponseChannel<FileResponse>,
    },
}

// Simple file exchange protocol

#[derive(Debug, Clone)]
pub struct FileExchangeProtocol();
#[derive(Clone)]
pub struct FileExchangeCodec();
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRequest(String);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResponse(String);

impl ProtocolName for FileExchangeProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/file-exchange/1".as_bytes()
    }
}

#[async_trait]
impl RequestResponseCodec for FileExchangeCodec {
    type Protocol = FileExchangeProtocol;
    type Request = FileRequest;
    type Response = FileResponse;

    async fn read_request<T>(
        &mut self,
        _: &FileExchangeProtocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, 1_000_000).await?;

        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        Ok(FileRequest(String::from_utf8(vec).unwrap()))
    }

    async fn read_response<T>(
        &mut self,
        _: &FileExchangeProtocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, 1_000_000).await?;

        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        Ok(FileResponse(String::from_utf8(vec).unwrap()))
    }

    async fn write_request<T>(
        &mut self,
        _: &FileExchangeProtocol,
        io: &mut T,
        FileRequest(data): FileRequest,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, data).await?;
        io.close().await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &FileExchangeProtocol,
        io: &mut T,
        FileResponse(data): FileResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, data).await?;
        io.close().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use async_std::task::spawn;
    use futures::channel::{mpsc::Sender, oneshot};
    use libp2p::{
        development_transport,
        gossipsub::{
            GossipsubConfigBuilder, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
        },
        request_response::ProtocolSupport,
    };
    use std::{iter, str::FromStr, time::Duration};

    async fn dummy_test_client() -> Result<(EventLoop, Sender<Command>, PeerId), Box<dyn Error>> {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from_public_key(&keypair.public());
        let transport = development_transport(keypair.clone()).await?;

        let config = GossipsubConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(ValidationMode::Strict)
            .build()
            .expect("Correct Config");

        let behaviour = ComposedBehaviour {
            gossipsub: Gossipsub::new(MessageAuthenticity::Signed(keypair), config)
                .expect("correct configuration"),
            kademlia: Kademlia::new(peer_id, MemoryStore::new(peer_id)),
            request_response: RequestResponse::new(
                FileExchangeCodec(),
                iter::once((FileExchangeProtocol(), ProtocolSupport::Full)),
                Default::default(),
            ),
        };

        let swarm = Swarm::new(transport, behaviour, peer_id.clone());

        let (command_sender, command_receiver) = mpsc::channel::<Command>(1);
        let (event_sender, _event_reveiver) = mpsc::channel::<Event>(1);

        Ok((
            EventLoop::new(swarm, command_receiver, event_sender),
            command_sender,
            peer_id,
        ))
    }

    async fn two_peer_scenario(
    ) -> Result<(Sender<Command>, Sender<Command>, PeerId, PeerId), Box<dyn Error>> {
        let (ev_provider, command_sender, peer_id_prov) = dummy_test_client().await?;
        spawn(ev_provider.run());
        let (ev_revc, command_sender_recv, peer_id_recv) = dummy_test_client().await?;
        spawn(ev_revc.run());
        Ok((
            command_sender,
            command_sender_recv,
            peer_id_prov,
            peer_id_recv,
        ))
    }

    #[tokio::test]
    async fn test_event_loop() -> Result<(), Box<dyn Error>> {
        let (eventloop, mut command_sender, _) = dummy_test_client().await?;
        spawn(eventloop.run());
        let (tx, rc) = oneshot::channel();
        command_sender
            .send(Command::StartListening {
                addr: "/ip4/0.0.0.0/tcp/0".parse()?,
                sender: tx,
            })
            .await?;
        let _ = rc.await?;
        Ok(())
    }
    #[tokio::test]
    async fn test_dial() -> Result<(), Box<dyn Error>> {
        let (mut command_sender, mut command_sender_recv, peer_id_prov, _) =
            two_peer_scenario().await?;
        let (tx, _) = oneshot::channel();

        command_sender
            .send(Command::StartListening {
                addr: "/ip4/0.0.0.0/tcp/0".parse()?,
                sender: tx,
            })
            .await?;

        let (tx, rc) = oneshot::channel();
        command_sender_recv
            .send(Command::Dial {
                peer_id: peer_id_prov,
                peer_addr: format!("/p2p/{}", peer_id_prov).parse()?,
                sender: tx,
            })
            .await?;
        let _ = rc.await?;

        Ok(())
    }
    #[tokio::test]
    async fn test_chat() -> Result<(), Box<dyn Error>> {
        let test_topic = Topic::new("test-chat");
        let (mut command_sender, mut command_sender_recv, peer_id_prov, _) =
            two_peer_scenario().await?;
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
                topic: test_topic.clone(),
                sender: tx,
            })
            .await?;
        let _ = rc.await?;

        let (tx, rc) = oneshot::channel();
        command_sender_recv
            .send(Command::StartListening {
                addr: "/ip4/0.0.0.0/tcp/0".parse()?,
                sender: tx,
            })
            .await?;
        let _ = rc.await?;
        let (tx, rc) = oneshot::channel();
        command_sender_recv
            .send(Command::Subscribe {
                topic: test_topic.clone(),
                sender: tx,
            })
            .await?;
        let _ = rc.await?;

        let (tx, rc) = oneshot::channel();
        command_sender_recv
            .send(Command::Dial {
                peer_id: peer_id_prov,
                peer_addr: format!("/p2p/{}", peer_id_prov).parse()?,
                sender: tx,
            })
            .await?;

        let _ = rc.await?;

        let (tx, rc) = oneshot::channel();
        command_sender_recv
            .send(Command::SendMessage {
                topic: test_topic.clone(),
                message: String::from_str("hi")?,
                sender: tx,
            })
            .await?;

        let _ = rc.await?;
        Ok(())
    }
}

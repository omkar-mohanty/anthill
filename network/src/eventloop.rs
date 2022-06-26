use super::commands::Command;
use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite, AsyncWriteExt};
use libp2p::core::either::EitherError;
use libp2p::core::either::EitherError;
use libp2p::kad::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaEvent, QueryId, QueryResult};
use libp2p::request_response::{RequestResponse, RequestResponseEvent};
use libp2p::swarm::{ConnectionHandlerUpgrErr, Swarm, SwarmEvent};
use libp2p::{
    core::{
        upgrade::{read_length_prefixed, write_length_prefixed},
        ProtocolName,
    },
    request_response::RequestResponseCodec,
    NetworkBehaviour, PeerId,
};
use std::io;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
};
use tokio::sync::mpsc::Receiver;
use tokio::sync::oneshot::Sender;

pub struct EventLoop {
    swarm: Swarm<SkyNetworkBehaviour>,
    command_receiver: Receiver<Command>,
    pending_dial: HashMap<PeerId, Sender<Result<(), Box<dyn Error>>>>,
    pending_start_providing: HashMap<QueryId, Sender<()>>,
    pending_get_providers: HashMap<QueryId, Sender<HashSet<PeerId>>>,
}

impl EventLoop {
    async fn handle_event(
        &mut self,
        event: SwarmEvent<
            SkyNetworkEvent,
            EitherError<ConnectionHandlerUpgrErr<io::Error>, io::Error>,
        >,
    ) {
        match event {
            SwarmEvent::Behaviour(SkyNetworkEvent::Kademlia(
                KademliaEvent::OutboundQueryCompleted {
                    id,
                    result: QueryResult::StartProviding(_),
                    ..
                },
            )) => {
                let sender: Sender<()> = self
                    .pending_start_providing
                    .remove(&id)
                    .expect("Completed query");
                let _ = sender.send(());
            }

            SwarmEvent::Behaviour(SkyNetworkEvent::Kademlia(KademliaEvent::OutboundQueryCompleted { id, result:QueryResult::GetProviders(_),..})) => {
                let sender: Sender<()> = self
                    .pending_get_providers
                    .remove(&id)
                    .expect("Completed query to be previously pending");
                sender.send(());
            }
        }
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "SkyNetworkEvent")]
struct SkyNetworkBehaviour {
    request_response: RequestResponse<FileExchangeCodec>,
    kademila: Kademlia<MemoryStore>,
}

enum SkyNetworkEvent {
    RequestResponse(RequestResponseEvent<FileRequest, FileResponse>),
    Kademlia(KademliaEvent),
}

impl From<RequestResponseEvent<FileRequest, FileResponse>> for SkyNetworkEvent {
    fn from(event: RequestResponseEvent<FileRequest, FileResponse>) -> Self {
        SkyNetworkEvent::RequestResponse(event)
    }
}

impl From<KademliaEvent> for SkyNetworkEvent {
    fn from(event: KademliaEvent) -> Self {
        SkyNetworkEvent::Kademlia(event)
    }
}

#[derive(Debug, Clone)]
struct FileExchangeProtocol();
#[derive(Clone)]
struct FileExchangeCodec();
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileRequest(String);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResponse(String);

impl ProtocolName for FileExchangeProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/sky-file-exchange".as_bytes()
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
        T: AsyncRead + Send + Unpin,
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
        T: AsyncRead + Send + Unpin,
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

use std::{collections::HashSet, pin::Pin, sync::Arc};

use anyhow::{Context, anyhow};
use futures::{
    SinkExt, StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
};
use log::{error, info};
use matchbox_socket::{Error as SocketError, PeerId, PeerState, WebRtcSocket};
use tokio::{
    sync::{Mutex, mpsc},
    task::yield_now,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use manhunt_logic::{Transport, TransportMessage, prelude::*};

use crate::{packets::PacketHandler, server};

type QueuePair<T> = (mpsc::Sender<T>, Mutex<mpsc::Receiver<T>>);
type MsgPair = (Option<Uuid>, TransportMessage);
type Queue = QueuePair<MsgPair>;

pub struct MatchboxTransport {
    my_id: Uuid,
    incoming: Queue,
    all_peers: Mutex<HashSet<Uuid>>,
    msg_sender: UnboundedSender<MatchboxMsgPair>,
    cancel_token: CancellationToken,
}

type LoopFutRes = Result<(), SocketError>;

type MatchboxMsgPair = (PeerId, Box<[u8]>);

fn map_socket_error(err: SocketError) -> anyhow::Error {
    match err {
        SocketError::ConnectionFailed(e) => anyhow!("Connection to server failed: {e:?}"),
        SocketError::Disconnected(e) => anyhow!("Connection to server lost: {e:?}"),
    }
}

impl MatchboxTransport {
    pub async fn new(join_code: &str, is_host: bool) -> Result<Arc<Self>> {
        let (itx, irx) = mpsc::channel(15);

        let ws_url = server::room_url(join_code, is_host);

        let (mut socket, mut loop_fut) = WebRtcSocket::new_reliable(&ws_url);

        let (mtx, mrx) = socket
            .take_channel(0)
            .expect("Failed to get channel")
            .split();

        let res = loop {
            tokio::select! {
                id = Self::wait_for_id(&mut socket) => {
                    if let Some(id) = id {
                        break Ok(id);
                    }
                },
                res = &mut loop_fut => {
                    break Err(match res {
                        Ok(_) => anyhow!("Transport disconnected unexpectedly"),
                        Err(err) => map_socket_error(err)
                    });
                }
            }
        }
        .context("While trying to join the lobby");

        match res {
            Ok(my_id) => {
                let transport = Arc::new(Self {
                    my_id,
                    incoming: (itx, Mutex::new(irx)),
                    all_peers: Mutex::new(HashSet::with_capacity(5)),
                    msg_sender: mtx.clone(),
                    cancel_token: CancellationToken::new(),
                });

                tokio::spawn({
                    let transport = transport.clone();
                    async move {
                        transport.main_loop(socket, loop_fut, mrx).await;
                    }
                });

                Ok(transport)
            }
            Err(why) => {
                drop(mrx);
                mtx.close_channel();
                drop(socket);
                Err(why)
            }
        }
    }

    async fn wait_for_id(socket: &mut WebRtcSocket) -> Option<Uuid> {
        if let Some(id) = socket.id() {
            Some(id.0)
        } else {
            yield_now().await;
            None
        }
    }

    async fn push_incoming(&self, id: Option<Uuid>, msg: TransportMessage) {
        self.incoming
            .0
            .send((id, msg))
            .await
            .expect("Failed to push to incoming queue");
    }

    async fn main_loop(
        &self,
        mut socket: WebRtcSocket,
        loop_fut: Pin<Box<dyn Future<Output = LoopFutRes> + Send + 'static>>,
        mut mrx: UnboundedReceiver<MatchboxMsgPair>,
    ) {
        tokio::pin!(loop_fut);
        let mut packet_handler = PacketHandler::default();

        info!("Starting transport loop");

        let (should_await, msg) = loop {
            tokio::select! {
                biased;

                res = &mut loop_fut => {
                    info!("Transport-initiated disconnect");
                    break (false, match res {
                        Ok(_) => TransportMessage::Disconnected,
                        Err(e) => {
                            let msg = map_socket_error(e).to_string();
                            TransportMessage::Error(msg)
                        }
                    });
                }

                _ = self.cancel_token.cancelled() => {
                    info!("Logic-initiated disconnect");
                    break (true, TransportMessage::Disconnected);
                }

                Some((peer, state)) = socket.next() => {
                    info!("Handling peer {peer}: {state:?}");
                    self.handle_peer(peer, state).await;
                }

                Some(data) = mrx.next() => {
                    info!("Handling new packet from {}", data.0);
                    self.handle_recv(data, &mut packet_handler).await;
                }


            }
        };

        self.push_incoming(Some(self.my_id), msg).await;

        self.msg_sender.close_channel();
        drop(mrx);
        socket.try_update_peers().ok();
        drop(socket);
        if should_await {
            if let Err(why) = loop_fut.await {
                error!("Failed to await after disconnect: {why:?}");
            }
        }
        info!("Transport disconnected");
    }

    async fn handle_peer(&self, peer: PeerId, state: PeerState) {
        let mut all_peers = self.all_peers.lock().await;
        let msg = match state {
            PeerState::Connected => {
                all_peers.insert(peer.0);
                TransportMessage::PeerConnect(peer.0)
            }
            PeerState::Disconnected => {
                all_peers.remove(&peer.0);
                TransportMessage::PeerDisconnect(peer.0)
            }
        };
        drop(all_peers);
        self.push_incoming(Some(peer.0), msg).await;
    }

    async fn handle_recv(
        &self,
        (PeerId(peer), packet): MatchboxMsgPair,
        handler: &mut PacketHandler,
    ) {
        match handler.consume_packet(peer, packet.into_vec()) {
            Ok(Some(msg)) => {
                self.push_incoming(Some(peer), msg).await;
            }
            Ok(None) => {
                // Non complete message
            }
            Err(why) => {
                error!("Error receiving message: {why}");
            }
        }
    }

    pub async fn send_transport_message(&self, peer: Option<Uuid>, msg: TransportMessage) {
        let mut tx = self.msg_sender.clone();

        match PacketHandler::message_to_packets(&msg) {
            Ok(packets) => {
                if let Some(peer) = peer {
                    let mut stream = futures::stream::iter(
                        packets
                            .into_iter()
                            .map(|p| Ok((PeerId(peer), p.into_boxed_slice()))),
                    );
                    if let Err(why) = tx.send_all(&mut stream).await {
                        error!("Error sending packet: {why}");
                    }
                } else {
                    let all_peers = self.all_peers.lock().await;
                    for peer in all_peers.iter().copied() {
                        let packets = packets.clone();
                        let mut stream = futures::stream::iter(
                            packets
                                .into_iter()
                                .map(|p| Ok((PeerId(peer), p.into_boxed_slice()))),
                        );
                        if let Err(why) = tx.send_all(&mut stream).await {
                            error!("Error sending packet: {why}");
                        }
                    }
                }
            }
            Err(why) => {
                error!("Error encoding message: {why}");
            }
        }
    }

    pub async fn recv_transport_messages(&self) -> Vec<MsgPair> {
        let mut incoming_rx = self.incoming.1.lock().await;
        let mut buffer = Vec::with_capacity(60);
        incoming_rx.recv_many(&mut buffer, 60).await;
        buffer
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }
}

impl Transport for MatchboxTransport {
    fn self_id(&self) -> Uuid {
        self.my_id
    }

    async fn receive_messages(&self) -> impl Iterator<Item = manhunt_logic::MsgPair> {
        self.recv_transport_messages().await.into_iter()
    }

    async fn send_message_single(&self, peer: Uuid, msg: TransportMessage) {
        self.send_transport_message(Some(peer), msg).await;
    }

    async fn send_message(&self, msg: TransportMessage) {
        self.send_transport_message(None, msg).await;
    }

    async fn send_self(&self, msg: TransportMessage) {
        self.push_incoming(Some(self.my_id), msg).await;
    }

    async fn room_joinable(&self, code: &str) -> bool {
        server::room_exists(code).await.unwrap_or(false)
    }

    async fn mark_room_started(&self, code: &str) {
        if let Err(why) = server::mark_room_started(code).await {
            error!("Failed to mark room {code} as started: {why:?}");
        }
    }

    async fn disconnect(&self) {
        self.cancel();
    }

    async fn initialize(code: &str, host: bool) -> Result<Arc<Self>> {
        Self::new(code, host).await
    }
}

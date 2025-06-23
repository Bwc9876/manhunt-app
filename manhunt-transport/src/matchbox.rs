use std::{pin::Pin, sync::Arc, time::Duration};

use anyhow::{Context, anyhow};
use futures::FutureExt;
use log::error;
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
    outgoing: Queue,
    cancel_token: CancellationToken,
}

type LoopFutRes = Result<(), SocketError>;

fn map_socket_error(err: SocketError) -> anyhow::Error {
    match err {
        SocketError::ConnectionFailed(e) => anyhow!("Connection to server failed: {e:?}"),
        SocketError::Disconnected(e) => anyhow!("Connection to server lost: {e:?}"),
    }
}

impl MatchboxTransport {
    pub async fn new(join_code: &str, is_host: bool) -> Result<Arc<Self>> {
        let (itx, irx) = mpsc::channel(15);
        let (otx, orx) = mpsc::channel(15);

        let ws_url = server::room_url(join_code, is_host);

        let (mut socket, mut loop_fut) = WebRtcSocket::new_reliable(&ws_url);

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
                    outgoing: (otx, Mutex::new(orx)),
                    cancel_token: CancellationToken::new(),
                });

                tokio::spawn({
                    let transport = transport.clone();
                    async move {
                        transport.main_loop(socket, loop_fut).await;
                    }
                });

                Ok(transport)
            }
            Err(why) => {
                drop(socket);
                loop_fut.await.context("While disconnecting")?;
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

    async fn push_many_incoming(&self, msgs: Vec<MsgPair>) {
        let senders = self
            .incoming
            .0
            .reserve_many(msgs.len())
            .await
            .expect("Failed to reserve in incoming queue");

        for (sender, msg) in senders.into_iter().zip(msgs.into_iter()) {
            sender.send(msg);
        }
    }

    async fn main_loop(
        &self,
        mut socket: WebRtcSocket,
        loop_fut: Pin<Box<dyn Future<Output = LoopFutRes> + Send + 'static>>,
    ) {
        let loop_fut = async {
            let msg = match loop_fut.await {
                Ok(_) => TransportMessage::Disconnected,
                Err(e) => {
                    let msg = map_socket_error(e).to_string();
                    TransportMessage::Error(msg)
                }
            };
            self.push_incoming(None, msg).await;
        }
        .fuse();

        tokio::pin!(loop_fut);

        let mut interval = tokio::time::interval(Duration::from_secs(1));

        let mut outgoing_rx = self.outgoing.1.lock().await;
        const MAX_MSG_SEND: usize = 30;
        let mut message_buffer = Vec::with_capacity(MAX_MSG_SEND);

        let mut packet_handler = PacketHandler::default();

        loop {
            self.handle_peers(&mut socket).await;

            self.handle_recv(&mut socket, &mut packet_handler).await;

            tokio::select! {
                biased;

                _ = self.cancel_token.cancelled() => {
                    break;
                }

                _ = &mut loop_fut => {
                    break;
                }

                _ = outgoing_rx.recv_many(&mut message_buffer, MAX_MSG_SEND) => {
                    let peers = socket.connected_peers().collect::<Vec<_>>();
                    self.handle_send(&mut socket, &peers, &mut message_buffer).await;
                }

                _ = interval.tick() => {
                    continue;
                }
            }
        }
    }

    async fn handle_peers(&self, socket: &mut WebRtcSocket) {
        for (peer, state) in socket.update_peers() {
            let msg = match state {
                PeerState::Connected => TransportMessage::PeerConnect(peer.0),
                PeerState::Disconnected => TransportMessage::PeerDisconnect(peer.0),
            };
            self.push_incoming(Some(peer.0), msg).await;
        }
    }

    async fn handle_send(
        &self,
        socket: &mut WebRtcSocket,
        all_peers: &[PeerId],
        messages: &mut Vec<MsgPair>,
    ) {
        let encoded_messages = messages.drain(..).filter_map(|(id, msg)| {
            match PacketHandler::message_to_packets(&msg) {
                Ok(packets) => Some((id, packets)),
                Err(why) => {
                    error!("Error encoding message to packets: {why:?}");
                    None
                }
            }
        });

        let channel = socket.channel_mut(0);

        for (peer, packets) in encoded_messages {
            if let Some(peer) = peer {
                for packet in packets {
                    channel.send(packet.into_boxed_slice(), PeerId(peer));
                }
            } else {
                for packet in packets {
                    let boxed = packet.into_boxed_slice();
                    for peer in all_peers {
                        channel.send(boxed.clone(), *peer);
                    }
                }
            }
        }
    }

    async fn handle_recv(&self, socket: &mut WebRtcSocket, handler: &mut PacketHandler) {
        let data = socket.channel_mut(0).receive();
        let messages = data
            .into_iter()
            .filter_map(
                |(peer, bytes)| match handler.consume_packet(peer.0, bytes.into_vec()) {
                    Ok(msg) => msg.map(|msg| (Some(peer.0), msg)),
                    Err(why) => {
                        error!("Error receiving message: {why}");
                        None
                    }
                },
            )
            .collect();
        self.push_many_incoming(messages).await;
    }

    pub async fn send_transport_message(&self, peer: Option<Uuid>, msg: TransportMessage) {
        self.outgoing
            .0
            .send((peer, msg))
            .await
            .expect("Failed to add to outgoing queue");
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

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use anyhow::Context;
use futures::FutureExt;
use log::error;
use matchbox_socket::{PeerId, PeerState, WebRtcSocket};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    game::{GameEvent, Transport},
    lobby::LobbyMessage,
    prelude::*,
    server,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransportChunk {
    id: u64,
    current: usize,
    total: usize,
    data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TransportMessage {
    /// The transport has received a peer id
    IdAssigned(Uuid),
    /// Message related to the actual game
    /// Boxed for space reasons
    Game(Box<GameEvent>),
    /// Message related to the pre-game lobby
    Lobby(Box<LobbyMessage>),
    /// Internal message when peer connects
    PeerConnect,
    /// Internal message when peer disconnects
    PeerDisconnect,
    /// Event sent when the transport gets disconnected, used to help consumers know when to stop
    /// consuming messages
    Disconnected,
    /// Internal message for packet chunking
    Seq(TransportChunk),
}

// Max packet size according to: https://github.com/johanhelsing/matchbox/issues/272
const MAX_PACKET_SIZE: usize = 65535;

// Align packets with a bit of extra space for [TransportMessage::Seq] header
const PACKET_ALIGNMENT: usize = MAX_PACKET_SIZE - 128;

impl TransportMessage {
    pub fn serialize(&self) -> Vec<u8> {
        rmp_serde::to_vec(self).expect("Failed to encode")
    }

    pub fn deserialize(data: &[u8]) -> Result<Self> {
        rmp_serde::from_slice(data).context("While deserializing message")
    }

    pub fn from_packets(packets: impl Iterator<Item = Box<[u8]>>) -> Result<Self> {
        let full_data = packets.flatten().collect::<Box<[u8]>>();
        Self::deserialize(&full_data).context("While decoding a multi-part message")
    }

    pub fn to_packets(&self) -> Vec<Vec<u8>> {
        let bytes = self.serialize();
        if bytes.len() > MAX_PACKET_SIZE {
            let id = rand::random_range(0..u64::MAX);
            let packets_needed = bytes.len().div_ceil(PACKET_ALIGNMENT);
            let rem = bytes.len() % PACKET_ALIGNMENT;
            let bytes = bytes.into_boxed_slice();
            (0..packets_needed)
                .map(|idx| {
                    let start = PACKET_ALIGNMENT * idx;
                    let end = if idx == packets_needed - 1 {
                        start + rem
                    } else {
                        PACKET_ALIGNMENT * (idx + 1)
                    };
                    let data = bytes[start..end].to_vec();
                    let chunk = TransportChunk {
                        id,
                        current: idx,
                        total: packets_needed,
                        data,
                    };
                    TransportMessage::Seq(chunk).serialize()
                })
                .collect()
        } else {
            vec![bytes]
        }
    }
}

impl From<GameEvent> for TransportMessage {
    fn from(v: GameEvent) -> Self {
        Self::Game(Box::new(v))
    }
}

impl From<LobbyMessage> for TransportMessage {
    fn from(v: LobbyMessage) -> Self {
        Self::Lobby(Box::new(v))
    }
}

type OutgoingMsgPair = (Option<Uuid>, TransportMessage);
type OutgoingQueueSender = tokio::sync::mpsc::Sender<OutgoingMsgPair>;
type OutgoingQueueReceiver = tokio::sync::mpsc::Receiver<OutgoingMsgPair>;

type IncomingMsgPair = (Uuid, TransportMessage);
type IncomingQueueSender = tokio::sync::mpsc::Sender<IncomingMsgPair>;
type IncomingQueueReceiver = tokio::sync::mpsc::Receiver<IncomingMsgPair>;

pub struct MatchboxTransport {
    ws_url: String,
    incoming: (IncomingQueueSender, Mutex<IncomingQueueReceiver>),
    outgoing: (OutgoingQueueSender, Mutex<OutgoingQueueReceiver>),
    my_id: RwLock<Option<Uuid>>,
    cancel_token: CancellationToken,
}

impl MatchboxTransport {
    pub fn new(join_code: &str, is_host: bool) -> Self {
        let (itx, irx) = tokio::sync::mpsc::channel(15);
        let (otx, orx) = tokio::sync::mpsc::channel(15);

        Self {
            ws_url: server::room_url(join_code, is_host),
            incoming: (itx, Mutex::new(irx)),
            outgoing: (otx, Mutex::new(orx)),
            my_id: RwLock::new(None),
            cancel_token: CancellationToken::new(),
        }
    }

    pub async fn send_transport_message(&self, peer: Option<Uuid>, msg: TransportMessage) {
        self.outgoing
            .0
            .send((peer, msg))
            .await
            .expect("Failed to add to outgoing queue");
    }

    pub async fn recv_transport_messages(&self) -> Vec<IncomingMsgPair> {
        let mut incoming_rx = self.incoming.1.lock().await;
        let mut buffer = Vec::with_capacity(60);
        incoming_rx.recv_many(&mut buffer, 60).await;
        buffer
    }

    async fn push_incoming(&self, id: Uuid, msg: TransportMessage) {
        self.incoming
            .0
            .send((id, msg))
            .await
            .expect("Failed to push to incoming queue");
    }

    async fn handle_send(
        &self,
        socket: &mut WebRtcSocket,
        all_peers: &HashSet<PeerId>,
        messages: &mut Vec<OutgoingMsgPair>,
    ) {
        if let Some(my_id) = *self.my_id.read().await {
            for (_, msg) in messages.iter().filter(|(id, _)| id.is_none()) {
                self.push_incoming(my_id, msg.clone()).await;
            }
        }

        let packets = messages.drain(..).flat_map(|(id, msg)| {
            msg.to_packets()
                .into_iter()
                .map(move |packet| (id, packet.into_boxed_slice()))
        });

        for (peer, packet) in packets {
            if let Some(peer) = peer {
                let channel = socket.channel_mut(0);
                channel.send(packet, PeerId(peer));
            } else {
                let channel = socket.channel_mut(0);

                for peer in all_peers.iter() {
                    // TODO: Any way around having to clone here?
                    let data = packet.clone();
                    channel.send(data, *peer);
                }
            }
        }
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    pub async fn transport_loop(&self) {
        let (mut socket, loop_fut) = WebRtcSocket::new_reliable(&self.ws_url);

        let loop_fut = loop_fut.fuse();
        tokio::pin!(loop_fut);

        let mut all_peers = HashSet::<PeerId>::with_capacity(20);
        let mut my_id = None;

        let mut timer = tokio::time::interval(Duration::from_millis(100));

        let mut partial_packets =
            HashMap::<u64, (Uuid, HashMap<usize, Option<Vec<u8>>>)>::with_capacity(3);

        loop {
            for (peer, state) in socket.update_peers() {
                let msg = match state {
                    PeerState::Connected => {
                        all_peers.insert(peer);
                        TransportMessage::PeerConnect
                    }
                    PeerState::Disconnected => {
                        all_peers.remove(&peer);
                        TransportMessage::PeerDisconnect
                    }
                };
                self.push_incoming(peer.0, msg).await;
            }

            let messages = socket.channel_mut(0).receive();

            let mut messages = messages
                .into_iter()
                .filter_map(|(id, data)| {
                    let msg = TransportMessage::deserialize(&data).ok();

                    if let Some(TransportMessage::Seq(TransportChunk {
                        id: multipart_id,
                        current,
                        total,
                        data,
                    })) = msg
                    {
                        if let Some((_, map)) = partial_packets.get_mut(&multipart_id) {
                            map.insert(current, Some(data));
                        } else {
                            let mut map = HashMap::from_iter((0..total).map(|idx| (idx, None)));
                            map.insert(current, Some(data));
                            partial_packets.insert(multipart_id, (id.0, map));
                        }
                        None
                    } else {
                        msg.map(|msg| (id.0, msg))
                    }
                })
                .collect::<Vec<_>>();

            let complete_messages = partial_packets
                .keys()
                .copied()
                .filter(|id| {
                    partial_packets
                        .get(id)
                        .is_some_and(|(_, v)| v.values().all(Option::is_some))
                })
                .collect::<Vec<_>>();

            for id in complete_messages {
                let (peer, packet_map) = partial_packets.remove(&id).unwrap();

                let res = TransportMessage::from_packets(
                    packet_map
                        .into_values()
                        .map(|v| v.unwrap().into_boxed_slice()),
                );

                match res {
                    Ok(msg) => messages.push((peer, msg)),
                    Err(why) => error!("Error receiving message: {why:?}"),
                }
            }

            let push_iter = self
                .incoming
                .0
                .reserve_many(messages.len())
                .await
                .expect("Couldn't reserve space");

            for (sender, msg) in push_iter.zip(messages.into_iter()) {
                sender.send(msg);
            }

            if my_id.is_none() {
                if let Some(new_id) = socket.id() {
                    my_id = Some(new_id.0);
                    *self.my_id.write().await = Some(new_id.0);
                    self.push_incoming(new_id.0, TransportMessage::IdAssigned(new_id.0))
                        .await;
                }
            }

            let mut outgoing_rx = self.outgoing.1.lock().await;

            let mut buffer = Vec::with_capacity(30);

            tokio::select! {

                _ = self.cancel_token.cancelled() => {
                    socket.close();
                }

                _ = timer.tick() => {
                    // Transport Tick
                }

                _ = outgoing_rx.recv_many(&mut buffer, 30) => {

                    self.handle_send(&mut socket, &all_peers, &mut buffer).await;
                }

                _ = &mut loop_fut => {
                    // Break on disconnect
                    break;
                }
            }
        }
    }
}

impl Transport for MatchboxTransport {
    async fn receive_messages(&self) -> impl Iterator<Item = GameEvent> {
        self.recv_transport_messages()
            .await
            .into_iter()
            .filter_map(|(id, msg)| match msg {
                TransportMessage::Game(game_event) => Some(*game_event),
                TransportMessage::PeerDisconnect => Some(GameEvent::DroppedPlayer(id)),
                _ => None,
            })
    }

    async fn send_message(&self, msg: GameEvent) {
        self.send_transport_message(None, msg.into()).await;
    }

    fn disconnect(&self) {
        self.cancel();
    }
}

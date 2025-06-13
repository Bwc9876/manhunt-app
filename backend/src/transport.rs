use std::{collections::HashSet, time::Duration};

use futures::{FutureExt, SinkExt};
use matchbox_socket::{PeerId, PeerState, WebRtcSocket};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::{
    game::{GameEvent, Transport},
    lobby::LobbyMessage,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TransportMessage {
    /// Message related to the actual game
    Game(GameEvent<PeerId>),
    /// Message related to the pre-game lobby
    Lobby(LobbyMessage),
    /// Internal message when peer connects
    PeerConnect,
    /// Internal message when peer disconnects
    PeerDisconnect,
}

type OutgoingMsgPair = (Option<PeerId>, TransportMessage);
type OutgoingQueueSender = tokio::sync::mpsc::Sender<OutgoingMsgPair>;
type OutgoingQueueReceiver = tokio::sync::mpsc::Receiver<OutgoingMsgPair>;

type IncomingMsgPair = (PeerId, TransportMessage);
type IncomingQueueSender = tokio::sync::mpsc::Sender<IncomingMsgPair>;
type IncomingQueueReceiver = tokio::sync::mpsc::Receiver<IncomingMsgPair>;

pub struct MatchboxTransport {
    ws_url: String,
    incoming: (IncomingQueueSender, Mutex<IncomingQueueReceiver>),
    outgoing: (OutgoingQueueSender, Mutex<OutgoingQueueReceiver>),
    my_id: RwLock<Option<PeerId>>,
}

impl MatchboxTransport {
    pub fn new(ws_url: &str) -> Self {
        let (itx, irx) = tokio::sync::mpsc::channel(15);
        let (otx, orx) = tokio::sync::mpsc::channel(15);

        Self {
            ws_url: ws_url.to_string(),
            incoming: (itx, Mutex::new(irx)),
            outgoing: (otx, Mutex::new(orx)),
            my_id: RwLock::new(None),
        }
    }

    pub async fn send_transport_message(&self, peer: Option<PeerId>, msg: TransportMessage) {
        self.outgoing
            .0
            .send((peer, msg))
            .await
            .expect("Failed to add to outgoing queue");
    }

    pub async fn recv_transport_message(&self) -> Option<IncomingMsgPair> {
        let mut incoming_rx = self.incoming.1.lock().await;
        incoming_rx.recv().await
    }

    pub async fn get_my_id(&self) -> Option<PeerId> {
        *self.my_id.read().await
    }

    pub async fn transport_loop(&self) {
        let (mut socket, loop_fut) = WebRtcSocket::new_reliable(&self.ws_url);

        let loop_fut = loop_fut.fuse();
        tokio::pin!(loop_fut);

        let mut all_peers = HashSet::<PeerId>::with_capacity(20);
        let mut my_id = None;

        let mut timer = tokio::time::interval(Duration::from_millis(100));

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
                self.incoming
                    .0
                    .send((peer, msg))
                    .await
                    .expect("Failed to push to incoming queue");
            }

            for (peer, data) in socket.channel_mut(0).receive() {
                if let Ok(msg) = rmp_serde::from_slice(&data) {
                    self.incoming
                        .0
                        .send((peer, msg))
                        .await
                        .expect("Failed to push to incoming queue");
                }
            }

            if my_id.is_none() {
                if let Some(new_id) = socket.id() {
                    my_id = Some(new_id);
                    *self.my_id.write().await = Some(new_id);
                }
            }

            let mut outgoing_rx = self.outgoing.1.lock().await;

            tokio::select! {
                _ = timer.tick() => {
                    // Transport Tick
                    continue;
                }

                Some((peer, msg)) = outgoing_rx.recv() => {
                    let encoded = rmp_serde::to_vec(&msg).unwrap();

                    if let Some(peer) = peer {
                        let channel = socket.channel_mut(0);
                        let data = encoded.into_boxed_slice();
                        channel.send(data, peer);
                    } else {
                        // Send to self as well
                        if let Some(myself) = my_id {
                            self.incoming.0.send((myself, msg)).await.expect("Failed to push to incoming queue");
                        }
                        let channel = socket.channel_mut(0);

                        for peer in all_peers.iter() {
                            // TODO: Any way around having to clone here?
                            let data = encoded.clone().into_boxed_slice();
                            channel.send(data, *peer);
                        }
                    }
                }

                _ = &mut loop_fut => {
                    // Break on disconnect
                    break;
                }
            }
        }
    }
}

impl Transport<PeerId> for MatchboxTransport {
    async fn receive_message(&self) -> Option<GameEvent<PeerId>> {
        self.recv_transport_message()
            .await
            .and_then(|(_, msg)| match msg {
                TransportMessage::Game(game_event) => Some(game_event),
                _ => None,
            })
    }

    async fn send_message(&self, msg: GameEvent<PeerId>) {
        let msg = TransportMessage::Game(msg);
        self.send_transport_message(None, msg).await;
    }
}

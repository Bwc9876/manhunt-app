use async_trait::async_trait;
use axum::{
    Error as AxumError,
    extract::{Path, ws::Message},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use futures::StreamExt;
use log::{debug, error, info, warn};
use matchbox_protocol::{JsonPeerEvent, PeerId, PeerRequest};
use matchbox_signaling::{
    ClientRequestError, NoCallbacks, SignalingError, SignalingServerBuilder, SignalingState,
    SignalingTopology, WsStateMeta,
    common_logic::{self, StateObj, parse_request},
};

use anyhow::Context;
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    result::Result as StdResult,
};
use tokio::sync::mpsc::UnboundedSender;

type Result<T = (), E = anyhow::Error> = StdResult<T, E>;

type RoomId = String;
type Sender = UnboundedSender<Result<Message, AxumError>>;

#[derive(Debug, Clone)]
struct Match {
    pub open_lobby: bool,
    pub players: HashSet<PeerId>,
}

#[derive(Debug, Clone)]
struct Peer {
    pub room: RoomId,
    sender: Sender,
}

impl Match {
    pub fn new() -> Self {
        Self {
            open_lobby: true,
            players: HashSet::with_capacity(10),
        }
    }
}

#[derive(Default, Debug, Clone)]
struct ServerState {
    waiting_clients: StateObj<HashMap<SocketAddr, RoomId>>,
    queued_clients: StateObj<HashMap<PeerId, RoomId>>,
    matches: StateObj<HashMap<RoomId, Match>>,
    clients: StateObj<HashMap<PeerId, Peer>>,
}

impl SignalingState for ServerState {}

impl ServerState {
    fn add_client(&mut self, origin: SocketAddr, code: RoomId) {
        self.waiting_clients
            .lock()
            .unwrap()
            .insert(origin, code.clone());
    }

    pub fn room_is_open(&self, room_id: RoomId) -> bool {
        self.matches
            .lock()
            .unwrap()
            .get(&room_id)
            .is_some_and(|m| m.open_lobby)
    }

    /// Mark a match as started, disallowing others from joining
    pub fn mark_started(&mut self, room: &RoomId) {
        if let Some(mat) = self.matches.lock().unwrap().get_mut(room) {
            mat.open_lobby = false;
        }
    }

    /// Create a new room with the given code, should be called when someone wants to host a game.
    /// Returns false if a room with that code already exists.
    pub fn create_room(&mut self, origin: SocketAddr, code: RoomId) -> bool {
        let mut matches = self.matches.lock().unwrap();
        if matches.contains_key(&code) {
            false
        } else {
            matches.insert(code.clone(), Match::new());
            drop(matches);
            self.add_client(origin, code);
            true
        }
    }

    /// Try to join a room by a code, returns `true` if successful
    pub fn try_join_room(&mut self, origin: SocketAddr, code: RoomId) -> bool {
        if self
            .matches
            .lock()
            .unwrap()
            .get(&code)
            .is_some_and(|m| m.open_lobby)
        {
            self.waiting_clients.lock().unwrap().insert(origin, code);
            true
        } else {
            false
        }
    }

    /// Assign a peer an id
    pub fn assign_peer_id(&mut self, origin: SocketAddr, peer_id: PeerId) {
        let target_room = self
            .waiting_clients
            .lock()
            .unwrap()
            .remove(&origin)
            .expect("origin not waiting?");

        self.queued_clients
            .lock()
            .unwrap()
            .insert(peer_id, target_room);
    }

    /// Add a peer to a room, returns other peers in the match currently
    pub fn add_peer(&mut self, peer_id: PeerId, sender: Sender) -> Vec<PeerId> {
        let target_room = self
            .queued_clients
            .lock()
            .unwrap()
            .remove(&peer_id)
            .expect("peer not waiting?");
        let mut matches = self.matches.lock().unwrap();
        let mat = matches.get_mut(&target_room).expect("Room not found?");
        let peers = mat.players.iter().copied().collect::<Vec<_>>();
        mat.players.insert(peer_id);
        drop(matches);
        let peer = Peer {
            room: target_room,
            sender,
        };
        self.clients.lock().unwrap().insert(peer_id, peer);
        peers
    }

    /// Disconnect a peer from a room. Automatically deletes the room if no peers remain. Returns
    /// the removed peer and the set of other peers in the room that need to be notified
    pub fn remove_peer(&mut self, peer_id: PeerId) -> Option<Vec<PeerId>> {
        let removed_peer = self.clients.lock().unwrap().remove(&peer_id)?;
        let other_peers = self
            .matches
            .lock()
            .unwrap()
            .get_mut(&removed_peer.room)
            .map(|m| {
                m.players.remove(&peer_id);
                m.players.iter().copied().collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if other_peers.is_empty() {
            self.matches.lock().unwrap().remove(&removed_peer.room);
        }

        Some(other_peers)
    }

    pub fn try_send(&self, peer: PeerId, msg: Message) -> Result<(), SignalingError> {
        self.clients
            .lock()
            .unwrap()
            .get(&peer)
            .ok_or(SignalingError::UnknownPeer)
            .and_then(|peer| common_logic::try_send(&peer.sender, msg))
    }
}

#[derive(Default, Debug)]
struct ServerTopology;

#[async_trait]
impl SignalingTopology<NoCallbacks, ServerState> for ServerTopology {
    async fn state_machine(upgrade: WsStateMeta<NoCallbacks, ServerState>) {
        let WsStateMeta {
            peer_id,
            sender,
            mut receiver,
            mut state,
            ..
        } = upgrade;

        let other_peers = state.add_peer(peer_id, sender.clone());

        let msg = Message::Text(JsonPeerEvent::NewPeer(peer_id).to_string().into());

        for other_id in other_peers {
            if let Err(why) = state.try_send(other_id, msg.clone()) {
                error!("Failed to publish new peer event to {other_id}: {why:?}");
            }
        }

        while let Some(req) = receiver.next().await {
            let req = match parse_request(req) {
                Ok(req) => req,
                Err(e) => match e {
                    ClientRequestError::Axum(e) => {
                        warn!("Peer {peer_id} encountered Axum error: {e:?}. Disconnecting...");
                        break;
                    }
                    ClientRequestError::Close => {
                        info!("Peer {peer_id} closed connection");
                        break;
                    }
                    ClientRequestError::Json(_) | ClientRequestError::UnsupportedType(_) => {
                        error!("Error parsing request from {peer_id}: {e:?}");
                        continue; // Recoverable, although may mean bad state?
                    }
                },
            };

            if let PeerRequest::Signal { receiver, data } = req {
                let msg = Message::Text(
                    JsonPeerEvent::Signal {
                        sender: peer_id,
                        data,
                    }
                    .to_string()
                    .into(),
                );
                if let Err(why) = state.try_send(receiver, msg) {
                    error!("Error sending signaling message from {peer_id} to {receiver}: {why:?}");
                }
            } // Other variant, PeerRequest::KeepAlive is just for a heartbeat, do nothing
        }

        info!("Peer {peer_id} disconnected");

        let msg = Message::Text(JsonPeerEvent::PeerLeft(peer_id).to_string().into());
        if let Some(other_peers) = state.remove_peer(peer_id) {
            for other_id in other_peers {
                if let Err(why) = state.try_send(other_id, msg.clone()) {
                    warn!("Failed to alert {other_id} that {peer_id} has disconnected: {why:?}");
                }
            }
        } else {
            warn!("Trying to remove peer {peer_id}, which doesn't exist?");
        }
    }
}

#[tokio::main]
async fn main() -> Result {
    colog::init();

    let args = std::env::args().collect::<Vec<_>>();
    let socket_addr = args
        .get(1)
        .map(|raw_binding| raw_binding.parse::<SocketAddr>())
        .transpose()
        .context("Invalid socket addr passed")?
        .unwrap_or(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 3536));

    let mut state = ServerState::default();

    let server = SignalingServerBuilder::new(socket_addr, ServerTopology, state.clone())
        .on_connection_request({
            let mut state = state.clone();
            move |connection| {
                info!("{} is requesting to connect", connection.origin);
                debug!("Connection meta: {connection:?}");

                let err = if let Some(room_code) = connection.path.clone() {
                    let is_host = connection.query_params.contains_key("create");
                    if is_host {
                        if state.create_room(connection.origin, room_code) {
                            None
                        } else {
                            Some(StatusCode::CONFLICT)
                        }
                    } else if state.try_join_room(connection.origin, room_code) {
                        None
                    } else {
                        Some(StatusCode::NOT_FOUND)
                    }
                } else {
                    Some(StatusCode::BAD_REQUEST)
                };

                if let Some(status) = err {
                    Err(status.into_response())
                } else {
                    Ok(true)
                }
            }
        })
        .mutate_router({
            let state = state.clone();
            move |router| {
                let mut state2 = state.clone();
                router
                    .route(
                        "/room_exists/{id}",
                        get(move |Path(room_id): Path<String>| async move {
                            if state.room_is_open(room_id) {
                                StatusCode::OK
                            } else {
                                StatusCode::NOT_FOUND
                            }
                        }),
                    )
                    .route(
                        "/mark_started/{id}",
                        get(move |Path(room_id): Path<String>| async move {
                            state2.mark_started(&room_id);
                            StatusCode::OK
                        }),
                    )
            }
        })
        .on_id_assignment({
            move |(socket, id)| {
                info!("Assigning id {id} to {socket}");
                state.assign_peer_id(socket, id);
            }
        })
        .build();

    info!(
        "Starting manhunt signaling server {}",
        env!("CARGO_PKG_VERSION")
    );

    server.serve().await.context("Error while running server")
}

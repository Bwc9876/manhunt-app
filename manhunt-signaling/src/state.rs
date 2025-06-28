use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
};

use axum::{Error as AxumError, extract::ws::Message, http::StatusCode};
use matchbox_protocol::PeerId;
use matchbox_signaling::{
    SignalingError, SignalingState,
    common_logic::{self, StateObj},
};
use rand::{rngs::ThreadRng, seq::IndexedRandom};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

pub type RoomId = String;
pub type Sender = UnboundedSender<Result<Message, AxumError>>;

#[derive(Debug, Clone)]
struct Match {
    pub open_lobby: bool,
    cancel: CancellationToken,
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
            cancel: CancellationToken::new(),
            players: HashSet::with_capacity(10),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct ServerState {
    waiting_clients: StateObj<HashMap<SocketAddr, (RoomId, bool)>>,
    queued_clients: StateObj<HashMap<PeerId, (RoomId, bool)>>,
    matches: StateObj<HashMap<RoomId, Match>>,
    clients: StateObj<HashMap<PeerId, Peer>>,
}

impl SignalingState for ServerState {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomError {
    /// Room already exists
    Exists,
    /// Room was not found
    NotFound,
}

impl From<RoomError> for StatusCode {
    fn from(val: RoomError) -> Self {
        match val {
            RoomError::Exists => StatusCode::CONFLICT,
            RoomError::NotFound => StatusCode::NOT_FOUND,
        }
    }
}

const ROOM_CODE_CHAR_POOL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";
const ROOM_CODE_LEN: usize = 6;
const MAX_ROOM_TRIES: usize = 25;

#[derive(Debug, Default, Clone, Copy)]
pub struct NoRoomsError;

impl ServerState {
    fn random_room_code(rng: &mut ThreadRng) -> RoomId {
        ROOM_CODE_CHAR_POOL
            .choose_multiple(rng, ROOM_CODE_LEN)
            .copied()
            .map(char::from)
            .collect()
    }

    fn check_room_taken(&self, code: &RoomId) -> bool {
        self.matches.lock().unwrap().contains_key(code)
    }

    pub fn generate_room_code(&self) -> Result<RoomId, NoRoomsError> {
        let mut rng = rand::rng();
        for _ in 0..MAX_ROOM_TRIES {
            let code = Self::random_room_code(&mut rng);

            if !self.check_room_taken(&code) {
                return Ok(code);
            }
        }
        Err(NoRoomsError)
    }

    fn add_client(&mut self, origin: SocketAddr, code: RoomId, host: bool) {
        self.waiting_clients
            .lock()
            .unwrap()
            .insert(origin, (code.clone(), host));
    }

    pub fn room_is_open(&self, room_id: &str) -> bool {
        self.matches
            .lock()
            .unwrap()
            .get(room_id)
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
    fn create_room(&mut self, origin: SocketAddr, code: RoomId) -> bool {
        let mut matches = self.matches.lock().unwrap();
        if matches.contains_key(&code) {
            false
        } else {
            matches.insert(code.clone(), Match::new());
            drop(matches);
            self.add_client(origin, code, true);
            true
        }
    }

    /// Try to join a room by a code, returns `true` if successful
    fn try_join_room(&mut self, origin: SocketAddr, code: RoomId) -> bool {
        if self.room_is_open(&code) {
            self.waiting_clients
                .lock()
                .unwrap()
                .insert(origin, (code, false));
            true
        } else {
            false
        }
    }

    /// Try to create / join a room
    pub fn handle_room(
        &mut self,
        create: bool,
        origin: SocketAddr,
        code: RoomId,
    ) -> Result<(), RoomError> {
        match create {
            true => match self.create_room(origin, code) {
                true => Ok(()),
                false => Err(RoomError::Exists),
            },
            false => match self.try_join_room(origin, code) {
                true => Ok(()),
                false => Err(RoomError::NotFound),
            },
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
    pub fn add_peer(
        &mut self,
        peer_id: PeerId,
        sender: Sender,
    ) -> (bool, CancellationToken, Vec<PeerId>) {
        let (target_room, host) = self
            .queued_clients
            .lock()
            .unwrap()
            .remove(&peer_id)
            .expect("peer not waiting?");
        let mut matches = self.matches.lock().unwrap();
        let mat = matches.get_mut(&target_room).expect("Room not found?");
        let peers = mat.players.iter().copied().collect::<Vec<_>>();
        mat.players.insert(peer_id);
        let cancel = mat.cancel.clone();
        drop(matches);
        let peer = Peer {
            room: target_room,
            sender,
        };
        self.clients.lock().unwrap().insert(peer_id, peer);
        (host, cancel, peers)
    }

    /// Disconnect a peer from a room. Automatically deletes the room if no peers remain. Returns
    /// the removed peer and the set of other peers in the room that need to be notified
    pub fn remove_peer(&mut self, peer_id: PeerId, host: bool) -> Option<Vec<PeerId>> {
        let removed_peer = self.clients.lock().unwrap().remove(&peer_id)?;

        let mut matches = self.matches.lock().unwrap();

        let other_peers = matches
            .get_mut(&removed_peer.room)
            .map(|m| {
                m.players.remove(&peer_id);
                m.players.iter().copied().collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if host {
            if let Some(mat) = matches.get_mut(&removed_peer.room).filter(|m| m.open_lobby) {
                // If we're host, disconnect everyone else
                mat.open_lobby = false;
                mat.cancel.cancel();
            }
        }

        if other_peers.is_empty() {
            matches.remove(&removed_peer.room);
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

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use uuid::Uuid;

    use super::*;

    const fn origin(p: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p)
    }

    const fn peer(p: u16) -> PeerId {
        PeerId(Uuid::from_u128(p as u128))
    }

    fn dummy_sender() -> Sender {
        let (s, _) = tokio::sync::mpsc::unbounded_channel();
        s
    }

    fn handle_assign_add(state: &mut ServerState, create: bool, code: &str, p: u16) {
        state
            .handle_room(create, origin(p), code.to_string())
            .expect("Failed to handle room");
        state.assign_peer_id(origin(p), peer(p));
        state.add_peer(peer(p), dummy_sender());
    }

    fn quick_create(state: &mut ServerState, code: &str, p: u16) {
        handle_assign_add(state, true, code, p);
    }

    fn quick_join(state: &mut ServerState, code: &str, p: u16) {
        handle_assign_add(state, false, code, p);
    }

    #[test]
    fn test_add_waiting_host() {
        let mut state = ServerState::default();

        let code = "aaa";

        state
            .handle_room(true, origin(1), code.to_string())
            .expect("Could not create room");
        assert_eq!(
            *state.waiting_clients.lock().unwrap(),
            HashMap::from_iter([(origin(1), (code.to_string(), true))])
        );
        assert!(state.room_is_open(code))
    }

    #[test]
    fn test_add_waiting_player() {
        let mut state = ServerState::default();

        let code = "aaaa";

        quick_create(&mut state, code, 1);

        state
            .handle_room(false, origin(2), code.to_string())
            .expect("Failed to join room");
        assert_eq!(
            *state.waiting_clients.lock().unwrap(),
            HashMap::from_iter([(origin(2), (code.to_string(), false))])
        );
    }

    #[test]
    fn test_assign_id() {
        let mut state = ServerState::default();

        let code = "aaa";

        state
            .handle_room(true, origin(1), code.to_string())
            .expect("Could not create room");

        state.assign_peer_id(origin(1), peer(1));

        assert!(state.waiting_clients.lock().unwrap().is_empty());
        assert_eq!(
            *state.queued_clients.lock().unwrap(),
            HashMap::from_iter([(peer(1), (code.to_string(), true))]),
        )
    }

    #[test]
    fn test_add_peer() {
        let mut state = ServerState::default();

        let code = "aaa";

        state
            .handle_room(true, origin(1), code.to_string())
            .expect("Could not create room");

        state.assign_peer_id(origin(1), peer(1));

        let (_, _, others) = state.add_peer(peer(1), dummy_sender());

        assert!(state.waiting_clients.lock().unwrap().is_empty());
        assert!(state.queued_clients.lock().unwrap().is_empty());
        assert!(others.is_empty());
        assert!(
            state
                .clients
                .lock()
                .unwrap()
                .get(&peer(1))
                .is_some_and(|p| p.room == code)
        );
        assert!(
            state
                .matches
                .lock()
                .unwrap()
                .get(&code.to_string())
                .is_some_and(|m| m.players.contains(&peer(1)))
        );
    }

    #[test]
    fn test_join_add_peer() {
        let mut state = ServerState::default();

        let code = "abcd";

        quick_create(&mut state, code, 1);

        state
            .handle_room(false, origin(2), code.to_string())
            .expect("Failed to join");
        state.assign_peer_id(origin(2), peer(2));

        let (_, _, others) = state.add_peer(peer(2), dummy_sender());

        assert_eq!(others, vec![peer(1)]);
        assert!(
            state
                .clients
                .lock()
                .unwrap()
                .get(&peer(2))
                .is_some_and(|p| p.room == code)
        );
        assert!(
            state
                .matches
                .lock()
                .unwrap()
                .get(&code.to_string())
                .is_some_and(|m| m.players.contains(&peer(1)) && m.players.contains(&peer(2)))
        );
    }

    #[test]
    fn test_player_leave() {
        let mut state = ServerState::default();

        let code = "asdfasdfasdfasdf";

        quick_create(&mut state, code, 1);
        quick_join(&mut state, code, 2);

        let others = state.remove_peer(peer(2), false);

        assert_eq!(others, Some(vec![peer(1)]));
        assert!(
            state
                .matches
                .lock()
                .unwrap()
                .get(&code.to_string())
                .is_some_and(|m| m.players.contains(&peer(1)) && !m.players.contains(&peer(2)))
        );
        assert!(!state.clients.lock().unwrap().contains_key(&peer(2)));
    }

    #[test]
    fn test_player_leave_only_one() {
        let mut state = ServerState::default();

        let code = "asdfasdfasdfasdf";

        quick_create(&mut state, code, 1);

        let others = state.remove_peer(peer(1), true);

        assert!(others.is_some_and(|v| v.is_empty()));
        assert!(state.matches.lock().unwrap().is_empty());
        assert!(state.clients.lock().unwrap().is_empty());
    }

    #[test]
    fn test_host_leave_with_players() {
        let mut state = ServerState::default();

        let code = "asdfasdfasdfasdf";

        quick_create(&mut state, code, 1);
        quick_join(&mut state, code, 2);

        let others = state.remove_peer(peer(1), true);

        assert_eq!(others, Some(vec![peer(2)]));
        let matches = state.matches.lock().unwrap();
        let mat = &matches[&code.to_string()];
        assert!(mat.cancel.is_cancelled());
        assert!(!mat.open_lobby);
    }

    #[test]
    fn test_host_leave_with_players_but_started() {
        let mut state = ServerState::default();

        let code = "asdfasdfasdfasdf";

        quick_create(&mut state, code, 1);
        quick_join(&mut state, code, 2);

        state.mark_started(&code.to_string());

        let others = state.remove_peer(peer(1), true);

        assert_eq!(others, Some(vec![peer(2)]));
        let matches = state.matches.lock().unwrap();
        let mat = &matches[&code.to_string()];
        assert!(!mat.cancel.is_cancelled());
        assert!(!mat.open_lobby);
    }

    #[test]
    fn test_join_no_match() {
        let mut state = ServerState::default();

        let code = "asdfasdf";

        let res = state.handle_room(false, origin(1), code.to_string());
        assert_eq!(res, Err(RoomError::NotFound));
    }

    #[test]
    fn test_create_exists() {
        let mut state = ServerState::default();

        let code = "cdf";

        quick_create(&mut state, code, 1);

        let res = state.handle_room(true, origin(2), code.to_string());
        assert_eq!(res, Err(RoomError::Exists));
    }

    #[test]
    fn test_join_started() {
        let mut state = ServerState::default();

        let code = "qwerty";

        quick_create(&mut state, code, 1);
        quick_join(&mut state, code, 2);

        state.mark_started(&code.to_string());

        assert!(
            state
                .matches
                .lock()
                .unwrap()
                .get(&code.to_string())
                .is_some_and(|m| !m.open_lobby)
        );

        let res = state.handle_room(false, origin(3), code.to_string());
        assert_eq!(res, Err(RoomError::NotFound));
    }
}

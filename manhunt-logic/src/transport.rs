use std::sync::Arc;

use super::{game_events::GameEvent, lobby::LobbyMessage};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TransportMessage {
    /// Message related to the actual game
    /// Boxed for space reasons
    Game(Box<GameEvent>),
    /// Message related to the pre-game lobby
    Lobby(Box<LobbyMessage>),
    /// Internal message when peer connects
    PeerConnect(Uuid),
    /// Internal message when peer disconnects
    PeerDisconnect(Uuid),
    /// Event sent when the transport gets disconnected, used to help consumers know when to stop
    /// consuming messages. Note this should represent a success state, the disconnect was
    /// triggered by user action.
    Disconnected,
    /// Event when the transport encounters a critical error and needs to disconnect.
    Error(String),
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

pub type MsgPair = (Option<Uuid>, TransportMessage);

pub trait Transport: Send + Sync {
    /// Start the transport loop, This is expected to spawn a new job that will loop until
    /// cancelled or an error occurs.
    fn initialize(
        code: &str,
        host: bool,
    ) -> impl std::future::Future<Output = Result<Arc<Self>, anyhow::Error>> + Send;
    /// Get the local user's ID
    fn self_id(&self) -> Uuid;
    /// Check if a room is open to join, non-host players will call this with a code
    fn room_joinable(&self, _code: &str) -> impl std::future::Future<Output = bool> + Send {
        async { true }
    }
    /// Request a room be marked unjoinable (due to a game starting), the host user will call this.
    fn mark_room_started(&self, _code: &str) -> impl Future<Output = ()> {
        async {}
    }
    /// Receive an event
    fn receive_messages(&self) -> impl Future<Output = impl Iterator<Item = MsgPair>>;
    /// Send a message to a specific peer
    fn send_message_single(&self, peer: Uuid, msg: TransportMessage) -> impl Future<Output = ()>;
    /// Send a message to all other peers
    fn send_message(&self, msg: TransportMessage) -> impl Future<Output = ()>;
    /// Send a message to the local user
    fn send_self(&self, msg: TransportMessage) -> impl Future<Output = ()>;
    /// Disconnect from the transport
    fn disconnect(&self) -> impl Future<Output = ()> {
        async {}
    }
}

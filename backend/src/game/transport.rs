use super::{events::GameEvent, PlayerId};

pub trait Transport<Id: PlayerId> {
    /// Receive an event
    async fn receive_message(&self) -> Option<GameEvent<Id>>;
    /// Send an event
    async fn send_message(&self, msg: GameEvent<Id>);
}

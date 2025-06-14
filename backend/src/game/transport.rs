use super::events::GameEvent;

pub trait Transport {
    /// Receive an event
    async fn receive_message(&self) -> Option<GameEvent>;
    /// Send an event
    async fn send_message(&self, msg: GameEvent);
}

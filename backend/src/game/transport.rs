use super::events::GameEvent;

pub trait Transport {
    /// Receive an event
    async fn receive_messages(&self) -> impl Iterator<Item = GameEvent>;
    /// Send an event
    async fn send_message(&self, msg: GameEvent);
    /// Disconnect from the transport
    fn disconnect(&self) {}
}

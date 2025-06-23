mod game;
mod game_events;
mod game_state;
mod lobby;
mod location;
mod powerups;
mod profile;
mod settings;
#[cfg(test)]
mod tests;
mod transport;

pub use game::{Game, StateUpdateSender};
pub use game_events::GameEvent;
pub use game_state::{GameHistory, GameUiState};
pub use lobby::{Lobby, LobbyMessage, LobbyState, StartGameInfo};
pub use location::{Location, LocationService};
pub use profile::PlayerProfile;
pub use settings::GameSettings;
pub use transport::{MsgPair, Transport, TransportMessage};

pub mod prelude {
    use anyhow::Error as AnyhowError;
    use std::result::Result as StdResult;
    pub type Result<T = (), E = AnyhowError> = StdResult<T, E>;
    pub use anyhow::Context;
}

use serde::{Deserialize, Serialize};

use super::{location::Location, state::PlayerPing, Id, UtcDT};

/// An event used between players to update state
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub enum GameEvent {
    /// A player has been caught and is now a seeker, contains the ID of the caught player
    PlayerCaught(Id),
    /// Public ping from a player revealing location
    Ping(PlayerPing),
    /// Force the player specified in `0` to ping, optionally display the ping as from the user
    /// specified in `1`.
    ForcePing(Id, Option<Id>),
    /// Force a powerup to despawn because a player got it, contains the player that got it.
    PowerupDespawn(Id),
    /// Contains location history of the given player, used after the game to sync location
    /// histories
    PostGameSync(Id, Vec<(UtcDT, Location)>),
    /// A player has been disconnected and removed from the game (because of error or otherwise).
    /// The player should be removed from all state
    DroppedPlayer(Id),
    /// The underlying transport has disconnected
    TransportDisconnect,
}

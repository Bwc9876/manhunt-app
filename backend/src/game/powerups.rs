use serde::{Deserialize, Serialize};

use super::{location::Location, PlayerId};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Type of powerup
pub enum PowerUpType {
    /// Ping a random seeker instead of a hider
    PingSeeker,

    /// Pings all seekers locations on the map for hiders
    PingAllSeekers,

    /// Ping another random hider instantly
    ForcePingOther,
}

impl PowerUpType {
    pub const ALL_TYPES: [Self; 3] = [
        PowerUpType::ForcePingOther,
        PowerUpType::PingAllSeekers,
        PowerUpType::PingSeeker,
    ];
}

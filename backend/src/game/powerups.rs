use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
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

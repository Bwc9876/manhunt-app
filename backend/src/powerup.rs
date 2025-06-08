use crate::state::{Location, PlayerId};

#[derive(Clone, Copy)]
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

#[derive(Clone)]
/// Usage of a powerup as reported to the host
pub enum PowerUpUsage {
    /// The hider will have their location replaced with a random seeker's
    PingSeeker,
    /// No additional args
    PingAllSeekers,
    /// Instantly ping another random hider, contains the unlucky person that is being pinged
    ForcePingOther(PlayerId),
}

#[derive(Clone, PartialEq, Eq)]
/// When a plugin is used
pub enum PowerUpTiming {
    /// Used the second it's activated
    Instant,
    /// Used during the next global ping
    NextPing,
}

impl PowerUpUsage {
    pub fn timing(&self) -> PowerUpTiming {
        match self {
            PowerUpUsage::PingSeeker => PowerUpTiming::NextPing,
            PowerUpUsage::ForcePingOther(_) => PowerUpTiming::Instant,
            PowerUpUsage::PingAllSeekers => PowerUpTiming::Instant,
        }
    }
}

#[derive(Clone)]
/// An on-map powerup that can be picked up by hiders
pub struct PowerUp {
    loc: Location,
    pub typ: PowerUpType,
}

impl PowerUp {
    pub fn new(loc: Location, typ: PowerUpType) -> Self {
        Self { loc, typ }
    }
}

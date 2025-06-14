use rand::distr::Bernoulli;
use serde::{Deserialize, Serialize};

use super::location::Location;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
/// The starting condition for global pings to begin
pub enum PingStartCondition {
    /// Wait For X players to be caught before beginning global pings
    Players(u32),
    /// Wait for X minutes after game start to begin global pings
    Minutes(u32),
    /// Don't wait at all, ping location after seekers are released
    Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
/// Settings for the game, host is the only person able to change these
pub struct GameSettings {
    /// The random seed used for shared rng
    pub random_seed: u32,
    /// The number of seconds to wait before seekers are allowed to go
    pub hiding_time_seconds: u32,
    /// Condition to wait for global pings to begin
    pub ping_start: PingStartCondition,
    /// Time between pings after the condition is met (first ping is either after the interval or
    /// instantly after the condition is met depending on the condition)
    pub ping_minutes_interval: u32,
    /// Condition for powerups to start spawning
    pub powerup_start: PingStartCondition,
    /// Chance every minute of a powerup spawning, out of 100
    pub powerup_chance: u32,
    /// Hard cooldown between powerups spawning
    pub powerup_minutes_cooldown: u32,
    /// Locations that powerups may spawn at
    pub powerup_locations: Vec<Location>,
}

impl GameSettings {
    pub fn get_powerup_bernoulli(&self) -> Bernoulli {
        Bernoulli::from_ratio(self.powerup_chance, 100).unwrap()
    }
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            random_seed: rand::random_range(0..=u32::MAX),
            hiding_time_seconds: 60,
            ping_start: PingStartCondition::Players(2),
            ping_minutes_interval: 3,
            powerup_start: PingStartCondition::Minutes(5),
            powerup_chance: 25,
            powerup_minutes_cooldown: 5,
            powerup_locations: vec![],
        }
    }
}

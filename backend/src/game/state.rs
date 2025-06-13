use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rand::{
    distr::{Bernoulli, Distribution},
    rngs::ThreadRng,
    seq::{IndexedRandom, IteratorRandom},
    Rng, SeedableRng,
};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};

use super::{
    location::Location,
    powerups::PowerUpType,
    settings::{GameSettings, PingStartCondition},
    PlayerId, UtcDT,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// An on-map ping of a player
pub struct PlayerPing<Id: PlayerId> {
    /// Location of the ping
    loc: Location,
    /// Time the ping happened
    timestamp: UtcDT,
    /// The player to display as
    pub display_player: Id,
    /// The actual player that initialized this ping
    pub real_player: Id,
}

impl<Id: PlayerId> PlayerPing<Id> {
    pub fn new(loc: Location, display_player: Id, real_player: Id) -> Self {
        Self {
            loc,
            display_player,
            real_player,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
/// Represents the game's state as a whole, seamlessly connects public and player state.
/// This struct handles all logic regarding state updates
pub struct GameState<Id: PlayerId> {
    /// The id of this player in this game
    pub id: Id,

    /// The powerup the player is currently holding
    held_powerup: Option<PowerUpType>,

    /// When the game started
    game_started: UtcDT,

    /// When seekers were allowed to begin
    seekers_started: Option<UtcDT>,

    /// Last time we pinged all players
    last_global_ping: Option<UtcDT>,

    /// Last time a powerup was spawned
    last_powerup_spawn: Option<UtcDT>,

    /// Hashmap tracking if a player is a seeker (true) or a hider (false)
    caught_state: HashMap<Id, bool>,

    /// A map of the latest global ping results for each player
    pings: HashMap<Id, PlayerPing<Id>>,

    /// Powerup on the map that players can grab. Only one at a time
    available_powerup: Option<Location>,

    /// The game's current settings
    settings: GameSettings,

    #[serde(skip)]
    /// The player's location history
    location_history: Vec<Location>,

    /// Cached bernoulli distribution for powerups, faster sampling
    #[serde(skip)]
    powerup_bernoulli: Bernoulli,

    /// A seed with a shared value between all players, should be reproducible
    /// RNG for use in stuff like powerup location selection.
    #[serde(skip)]
    shared_random_increment: i64,

    /// State for [ChaCha20Rng] to be used and added to when performing shared RNG operations
    #[serde(skip)]
    shared_random_state: u64,
}

impl<Id: PlayerId> GameState<Id> {
    pub fn new(settings: GameSettings, my_id: Id, initial_caught_state: HashMap<Id, bool>) -> Self {
        let mut rand = ChaCha20Rng::seed_from_u64(settings.random_seed);
        let increment = rand.random_range(-100..100);

        Self {
            id: my_id,
            game_started: Utc::now(),
            seekers_started: None,
            pings: HashMap::with_capacity(initial_caught_state.len()),
            caught_state: initial_caught_state,
            available_powerup: None,
            powerup_bernoulli: settings.get_powerup_bernoulli(),
            shared_random_state: settings.random_seed,
            settings,
            last_global_ping: None,
            last_powerup_spawn: None,
            location_history: Vec::with_capacity(30),
            held_powerup: None,
            shared_random_increment: increment,
        }
    }

    fn create_rand_from_shared_seed(&mut self) -> ChaCha20Rng {
        let rand = ChaCha20Rng::seed_from_u64(self.shared_random_state);

        self.shared_random_state = self
            .shared_random_state
            .wrapping_add_signed(self.shared_random_increment);

        rand
    }

    /// Spawn a powerup on the map, this **MUST** be called on all players at about the same time.
    /// First rolls to see if we will spawn one with `chance` (chance is percent chance out of 100).
    /// If the roll succeeds, spawn a powerup at one of the given locations.
    pub fn try_spawn_powerup(&mut self, now: UtcDT) {
        let mut shared_rand = self.create_rand_from_shared_seed();
        let roll = self.powerup_bernoulli.sample(&mut shared_rand);
        if roll {
            let choice = self
                .settings
                .powerup_locations
                .choose(&mut shared_rand)
                .cloned();
            self.available_powerup = choice;
            self.last_powerup_spawn = Some(now);
        }
    }

    fn minutes_since_seekers_released(&self, now: UtcDT) -> Option<u32> {
        self.seekers_started
            .as_ref()
            .map(|released| (now - *released).num_minutes().unsigned_abs() as u32)
    }

    pub fn pings_started(&self) -> bool {
        self.last_global_ping.is_some()
    }

    pub fn should_start_pings(&self, now: UtcDT) -> bool {
        match self.settings.ping_start {
            PingStartCondition::Players(num) => (self.iter_seekers().count() as u32) >= num,
            PingStartCondition::Minutes(minutes) => self
                .minutes_since_seekers_released(now)
                .is_some_and(|seekers_released| seekers_released >= minutes),
            PingStartCondition::Instant => true,
        }
    }

    /// Whether enough time has passed that we should perform a ping
    pub fn should_ping(&self, now: &UtcDT) -> bool {
        !self.is_seeker()
            && self.last_global_ping.as_ref().is_some_and(|last_ping| {
                let minutes = (*now - *last_ping).num_minutes().unsigned_abs();
                minutes >= self.settings.ping_minutes_interval
            })
    }

    /// Begin pinging, will start the countdown for global pings. Also refreshes the timeout
    pub fn start_pings(&mut self, now: UtcDT) {
        self.last_global_ping = Some(now);
    }

    /// Begin spawning powerups
    pub fn start_powerups(&mut self, now: UtcDT) {
        self.last_powerup_spawn = Some(now);
    }

    /// Whether to start spawning powerups
    pub fn should_start_powerups(&self, now: UtcDT) -> bool {
        match self.settings.powerup_start {
            PingStartCondition::Players(num) => (self.iter_seekers().count() as u32) >= num,
            PingStartCondition::Minutes(mins) => self
                .minutes_since_seekers_released(now)
                .is_some_and(|seekers_released| seekers_released >= mins),
            PingStartCondition::Instant => true,
        }
    }

    pub fn powerups_started(&self) -> bool {
        self.last_powerup_spawn.is_some()
    }

    /// Whether enough time has passed that we should roll for powerup spawns
    pub fn should_spawn_powerup(&self, now: &UtcDT) -> bool {
        self.last_powerup_spawn.as_ref().is_some_and(|last_spawn| {
            let minutes = (*now - *last_spawn).num_minutes().unsigned_abs();
            minutes >= self.settings.powerup_minutes_cooldown
        })
    }

    pub fn powerup_location(&self) -> Option<Location> {
        self.available_powerup
    }

    /// Despawn a powerup (due to timeout, other person getting it)
    pub fn despawn_powerup(&mut self) {
        self.available_powerup = None;
    }

    pub fn should_release_seekers(&self, now: UtcDT) -> bool {
        let seconds = (now - self.game_started).num_seconds().unsigned_abs();
        seconds >= (self.settings.hiding_time_seconds as u64)
    }

    /// Mark seekers as released
    pub fn release_seekers(&mut self, now: UtcDT) {
        self.seekers_started = Some(now);
    }

    /// If seekers are released
    pub fn seekers_released(&self) -> bool {
        self.seekers_started.is_some()
    }

    /// Add a ping for a specific player
    pub fn add_ping(&mut self, ping: PlayerPing<Id>) {
        self.pings.insert(ping.display_player, ping);
    }

    /// Get a ping for a player
    pub fn get_ping(&self, player: Id) -> Option<&PlayerPing<Id>> {
        self.pings.get(&player)
    }

    /// Remove a ping from the map
    pub fn remove_ping(&mut self, player: Id) -> Option<PlayerPing<Id>> {
        self.pings.remove(&player)
    }

    /// Iterate over all seekers in the game
    pub fn iter_seekers(&self) -> impl Iterator<Item = Id> + use<'_, Id> {
        self.caught_state
            .iter()
            .filter_map(|(k, v)| if *v { Some(*k) } else { None })
    }

    /// Pick a random seeker
    pub fn random_seeker(&mut self) -> Option<Id> {
        let seekers = self.iter_seekers().collect::<Vec<_>>();
        let mut rand = rand::rng();
        seekers.choose(&mut rand).copied()
    }

    /// Iterate over all hiders in the game
    fn iter_hiders(&self) -> impl Iterator<Item = Id> + use<'_, Id> {
        self.caught_state
            .iter()
            .filter_map(|(k, v)| if !*v { Some(*k) } else { None })
    }

    pub fn random_other_hider(&self) -> Option<Id> {
        let mut rand = rand::rng();
        self.iter_hiders()
            .filter(|id| *id != self.id)
            .choose(&mut rand)
    }

    /// Create a [PlayerPing] with the latest location saved for the player
    pub fn create_self_ping(&self) -> Option<PlayerPing<Id>> {
        self.create_ping(self.id)
    }

    /// Create a [PlayerPing] with the latest location as another player
    pub fn create_ping(&self, id: Id) -> Option<PlayerPing<Id>> {
        self.get_loc()
            .map(|loc| PlayerPing::new(loc.clone(), id, self.id))
    }

    /// Player has gotten a powerup, rolls to see which powerup and stores it
    pub fn get_powerup(&mut self) {
        let mut rand = rand::rng();
        // TODO: Seekers vs Hiders, Weights?
        let choice = PowerUpType::ALL_TYPES.choose(&mut rand).copied();
        self.held_powerup = choice;
    }

    pub fn force_set_powerup(&mut self, typ: PowerUpType) {
        self.held_powerup = Some(typ);
    }

    pub fn peek_powerup(&self) -> Option<&PowerUpType> {
        self.held_powerup.as_ref()
    }

    /// "Use" a powerup, takes it out of [held_powerup] and returns the type for use in game logic
    pub fn use_powerup(&mut self) -> Option<PowerUpType> {
        self.held_powerup.take()
    }

    /// Push a new player location
    pub fn push_loc(&mut self, loc: Location) {
        self.location_history.push(loc);
    }

    /// Get the latest player location
    fn get_loc(&self) -> Option<&Location> {
        self.location_history.last()
    }

    /// Mark a player as caught
    pub fn mark_caught(&mut self, player: Id) {
        if let Some(caught) = self.caught_state.get_mut(&player) {
            *caught = true;
        }
    }

    /// Gets if a player was caught or not
    pub fn get_caught(&self, player: Id) -> Option<bool> {
        self.caught_state.get(&player).copied()
    }

    pub fn is_seeker(&self) -> bool {
        self.caught_state.get(&self.id).copied().unwrap_or_default()
    }
}

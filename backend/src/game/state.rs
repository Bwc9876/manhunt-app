use std::collections::HashMap;

use chrono::Utc;
use rand::{
    distr::{Bernoulli, Distribution},
    seq::{IndexedRandom, IteratorRandom},
    Rng, SeedableRng,
};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::game::GameEvent;

use super::{
    location::Location,
    powerups::PowerUpType,
    settings::{GameSettings, PingStartCondition},
    Id, UtcDT,
};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
/// An on-map ping of a player
pub struct PlayerPing {
    /// Location of the ping
    loc: Location,
    /// Time the ping happened
    timestamp: UtcDT,
    /// The player to display as
    pub display_player: Id,
    /// The actual player that initialized this ping
    pub real_player: Id,
}

impl PlayerPing {
    pub fn new(loc: Location, display_player: Id, real_player: Id) -> Self {
        Self {
            loc,
            display_player,
            real_player,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone)]
/// This struct handles all logic regarding state updates
pub struct GameState {
    /// The id of this player in this game
    pub id: Id,

    /// The powerup the player is currently holding
    held_powerup: Option<PowerUpType>,

    /// When the game started
    game_started: UtcDT,

    /// When the game ended, if this is [Option::Some] then the state will enter post-game sync
    game_ended: Option<UtcDT>,

    /// A HashMap of player IDs to location histories, used to track all player location histories
    /// during post-game sync
    player_histories: HashMap<Uuid, Option<Vec<(UtcDT, Location)>>>,

    /// When seekers were allowed to begin
    seekers_started: Option<UtcDT>,

    /// Last time we pinged all players
    last_global_ping: Option<UtcDT>,

    /// Last time a powerup was spawned
    last_powerup_spawn: Option<UtcDT>,

    /// Hashmap tracking if a player is a seeker (true) or a hider (false)
    caught_state: HashMap<Id, bool>,

    /// A map of the latest global ping results for each player
    pings: HashMap<Id, PlayerPing>,

    /// Powerup on the map that players can grab. Only one at a time
    available_powerup: Option<Location>,

    pub event_history: Vec<(UtcDT, GameEvent)>,

    /// The game's current settings
    settings: GameSettings,

    /// The player's location history
    pub location_history: Vec<(UtcDT, Location)>,

    /// Cached bernoulli distribution for powerups, faster sampling
    powerup_bernoulli: Bernoulli,

    /// A seed with a shared value between all players, should be reproducible
    /// RNG for use in stuff like powerup location selection.
    shared_random_increment: i64,

    /// State for [ChaCha20Rng] to be used and added to when performing shared RNG operations
    shared_random_state: u64,
}

impl GameState {
    pub fn new(settings: GameSettings, my_id: Id, initial_caught_state: HashMap<Id, bool>) -> Self {
        let mut rand = ChaCha20Rng::seed_from_u64(settings.random_seed as u64);
        let increment = rand.random_range(-100..100);

        Self {
            id: my_id,
            game_started: Utc::now(),
            event_history: Vec::with_capacity(15),
            game_ended: None,
            seekers_started: None,
            pings: HashMap::with_capacity(initial_caught_state.len()),
            player_histories: HashMap::from_iter(initial_caught_state.keys().map(|id| (*id, None))),
            caught_state: initial_caught_state,
            available_powerup: None,
            powerup_bernoulli: settings.get_powerup_bernoulli(),
            shared_random_state: settings.random_seed as u64,
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
                minutes >= (self.settings.ping_minutes_interval as u64)
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
        if self.settings.powerup_locations.is_empty() {
            false
        } else {
            match self.settings.powerup_start {
                PingStartCondition::Players(num) => (self.iter_seekers().count() as u32) >= num,
                PingStartCondition::Minutes(mins) => self
                    .minutes_since_seekers_released(now)
                    .is_some_and(|seekers_released| seekers_released >= mins),
                PingStartCondition::Instant => true,
            }
        }
    }

    pub fn powerups_started(&self) -> bool {
        self.last_powerup_spawn.is_some()
    }

    /// Whether enough time has passed that we should roll for powerup spawns
    pub fn should_spawn_powerup(&self, now: &UtcDT) -> bool {
        self.last_powerup_spawn.as_ref().is_some_and(|last_spawn| {
            let minutes = (*now - *last_spawn).num_minutes().unsigned_abs();
            minutes >= (self.settings.powerup_minutes_cooldown as u64)
        })
    }

    #[cfg(test)]
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
    pub fn add_ping(&mut self, ping: PlayerPing) {
        self.pings.insert(ping.display_player, ping);
    }

    /// Get a ping for a player
    #[cfg(test)]
    pub fn get_ping(&self, player: Id) -> Option<&PlayerPing> {
        self.pings.get(&player)
    }

    /// Add a location history for the given player
    pub fn insert_player_location_history(&mut self, id: Uuid, history: Vec<(UtcDT, Location)>) {
        self.player_histories.insert(id, Some(history));
    }

    /// Check if we've complete the post-game sync
    pub fn check_post_game_sync(&self) -> bool {
        self.game_ended() && self.player_histories.values().all(Option::is_some)
    }

    /// Check if the game should be ended (due to all players being caught)
    pub fn check_end_game(&mut self) -> bool {
        let should_end = self.caught_state.values().all(|v| *v);
        if should_end {
            self.game_ended = Some(Utc::now());
            self.player_histories
                .insert(self.id, Some(self.location_history.clone()));
        }
        should_end
    }

    pub fn game_ended(&self) -> bool {
        self.game_ended.is_some()
    }

    /// Remove a ping from the map
    pub fn remove_ping(&mut self, player: Id) -> Option<PlayerPing> {
        self.pings.remove(&player)
    }

    /// Iterate over all seekers in the game
    pub fn iter_seekers(&self) -> impl Iterator<Item = Id> + use<'_> {
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
    fn iter_hiders(&self) -> impl Iterator<Item = Id> + use<'_> {
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
    pub fn create_self_ping(&self) -> Option<PlayerPing> {
        self.create_ping(self.id)
    }

    /// Create a [PlayerPing] with the latest location as another player
    pub fn create_ping(&self, id: Id) -> Option<PlayerPing> {
        self.get_loc().map(|loc| PlayerPing::new(*loc, id, self.id))
    }

    /// Remove a player from the game by their ID number
    pub fn remove_player(&mut self, id: Id) {
        self.pings.remove(&id);
        self.caught_state.remove(&id);
        self.player_histories.remove(&id);
    }

    /// Player has gotten a powerup, rolls to see which powerup and stores it
    pub fn get_powerup(&mut self) {
        let mut rand = rand::rng();
        // TODO: Seekers vs Hiders, Weights?
        let choice = PowerUpType::ALL_TYPES.choose(&mut rand).copied();
        self.held_powerup = choice;
    }

    #[cfg(test)]
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
        self.location_history.push((Utc::now(), loc));
    }

    /// Get the latest player location
    fn get_loc(&self) -> Option<&Location> {
        self.location_history.last().map(|(_, l)| l)
    }

    /// Mark a player as caught
    pub fn mark_caught(&mut self, player: Id) {
        if let Some(caught) = self.caught_state.get_mut(&player) {
            *caught = true;
        }
    }

    /// Gets if a player was caught or not
    #[cfg(test)]
    pub fn get_caught(&self, player: Id) -> Option<bool> {
        self.caught_state.get(&player).copied()
    }

    pub fn is_seeker(&self) -> bool {
        self.caught_state.get(&self.id).copied().unwrap_or_default()
    }

    pub fn as_game_history(&self) -> GameHistory {
        GameHistory {
            my_id: self.id,
            events: self.event_history.clone(),
            locations: self
                .player_histories
                .iter()
                .map(|(id, history)| (*id, history.as_ref().cloned().unwrap_or_default()))
                .collect(),
            game_started: self.game_started,
            game_ended: self.game_ended.unwrap_or_default(),
        }
    }

    pub fn as_ui_state(&self) -> GameUiState {
        GameUiState {
            my_id: self.id,
            caught_state: self.caught_state.clone(),
            available_powerup: self.available_powerup,
            pings: self.pings.clone(),
            game_started: self.game_started,
            game_ended: self.game_ended,
            last_global_ping: self.last_global_ping,
            last_powerup_spawn: self.last_powerup_spawn,
            held_powerup: self.held_powerup,
            seekers_started: self.seekers_started,
        }
    }

    pub fn clone_settings(&self) -> GameSettings {
        self.settings.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct GameHistory {
    my_id: Uuid,
    pub game_started: UtcDT,
    game_ended: UtcDT,
    events: Vec<(UtcDT, GameEvent)>,
    locations: Vec<(Uuid, Vec<(UtcDT, Location)>)>,
}

/// Subset of [GameState] that is meant to be sent to a UI frontend
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct GameUiState {
    /// ID of the local player
    my_id: Uuid,
    /// A map of player IDs to whether that player is a seeker
    caught_state: HashMap<Uuid, bool>,
    /// A powerup that is available on the map
    available_powerup: Option<Location>,
    /// A map of player IDs to an active ping on them
    pings: HashMap<Uuid, PlayerPing>,
    /// When the game was started **in UTC**
    game_started: UtcDT,
    /// When the game ended, when this is Option::Some, the game has ended
    game_ended: Option<UtcDT>,
    /// The last time all hiders were pinged **in UTC**
    last_global_ping: Option<UtcDT>,
    /// The last time a powerup was spawned **in UTC**
    last_powerup_spawn: Option<UtcDT>,
    /// The [PowerUpType] the local player is holding
    held_powerup: Option<PowerUpType>,
    /// When the seekers were allowed to start **in UTC**
    seekers_started: Option<UtcDT>,
}

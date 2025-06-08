use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::{
    game::LocationService,
    powerup::{PowerUp, PowerUpTiming, PowerUpType, PowerUpUsage},
};

/// UTC DateTime;
pub type DT = DateTime<Utc>;

/// Type used to uniquely identify players in the game
pub type PlayerId = u32;

/// Type used for latitude and longitude
pub type LocationComponent = f64;

#[derive(Debug, Clone)]
/// The starting condition for global pings to begin
pub enum PingStartCondition {
    /// Wait For X players to be caught before beginning global pings
    Players(u32),
    /// Wait for X minutes after game start to begin global pings
    Minutes(u32),
    /// Don't wait at all, ping location after seekers are released
    Instant,
}

#[derive(Debug, Clone)]
/// Settings for the game, host is the only person able to change these
pub struct GameSettings {
    /// The number of seconds to wait before seekers are allowed to go
    pub hiding_time_seconds: u64,
    /// Condition to wait for global pings to begin
    pub ping_start: PingStartCondition,
    /// Time between pings after the condition is met (first ping is either after the interval or
    /// instantly after the condition is met depending on the condition)
    pub ping_minutes_interval: u64,
    /// Condition for powerups to start spawning
    pub powerup_start: PingStartCondition,
    /// Chance (after cooldown) each minute of a powerup spawning, out of 100
    pub powerup_chance: u32,
    /// Hard cooldown between powerups spawning
    pub powerup_minutes_cooldown: u32,
    /// Locations that powerups may spawn at
    pub powerup_locations: Vec<Location>,
}

#[derive(Debug, Clone, Copy)]
/// Some location in the world as gotten from the Geolocation API
pub struct Location {
    /// Latitude
    pub lat: LocationComponent,
    /// Longitude
    pub long: LocationComponent,
    /// The bearing (float normalized from 0 to 1) optional as GPS can't always determine
    pub heading: Option<LocationComponent>,
}

/// State for each player during the game, the host also has this
pub struct PlayerState {
    /// The id of this player in this game
    pub id: PlayerId,
    /// All previous locations of this player, used in replay screen and when a ping happens
    pub locations: Vec<Location>,
    /// Whether the local player is a seeker
    pub seeker: bool,
    /// The powerup the player is currently holding
    pub held_powerup: Option<PowerUpType>,
}

/// Host state that determines when "privileged" events happen
pub struct HostState {
    /// The last time a location global ping occurred. If this is [Option::None] it means we're not
    /// pinging yet
    pub last_ping: Option<DT>,

    /// The last time a power-up has spawned.
    pub last_powerup: Option<DT>,

    /// Last time a roll was done for a powerup to spawn, if this is [Option::None] it means we're
    /// not spawning powerups yet.
    pub last_powerup_proc: Option<DT>,

    /// Set of users that will not be pinged / ping someone else next ping
    pub ping_power_usages: HashMap<PlayerId, PowerUpUsage>,

    /// A list of all events that happened in this game, and their times
    pub event_history: Vec<(PlayerId, DT, GameEvent)>,
}

impl HostState {
    pub fn new() -> Self {
        Self {
            last_ping: None,
            last_powerup: None,
            last_powerup_proc: None,
            ping_power_usages: HashMap::with_capacity(4),
            event_history: Vec::with_capacity(50),
        }
    }
}

#[derive(Clone)]
/// An on-map ping of a player
pub struct PlayerPing {
    /// Location of the ping
    loc: Location,
    /// Time the ping happened
    time: DT,
    /// The player to display who initialized this ping
    player: PlayerId,
    /// The actual player that initialized this ping
    real_player: PlayerId,
}

impl PlayerPing {
    pub fn new(loc: Location, player: PlayerId, real_player: PlayerId) -> Self {
        Self {
            loc,
            player,
            real_player,
            time: Utc::now(),
        }
    }
}

/// State meant to be updated and synced
pub struct PublicState {
    /// When the game started
    pub game_started: DT,

    /// When seekers were allowed to begin
    pub seekers_started: Option<DT>,

    /// Hashmap tracking if a player is a seeker (true) or a hider (false)
    pub caught_state: HashMap<PlayerId, bool>,

    /// A map of the latest global ping results for each player
    pub pings: HashMap<PlayerId, Option<PlayerPing>>,

    /// Powerup on the map that players can grab. Only one at a time
    pub available_powerup: Option<PowerUp>,
}

impl PublicState {
    pub fn new(players: HashMap<PlayerId, bool>) -> Self {
        Self {
            game_started: Utc::now(),
            seekers_started: None,
            pings: HashMap::from_iter(players.keys().map(|id| (*id, None))),
            caught_state: players,
            available_powerup: None,
        }
    }

    pub fn iter_seekers(&self) -> impl Iterator<Item = PlayerId> + use<'_> {
        self.caught_state
            .iter()
            .filter_map(|(k, v)| if *v { Some(*k) } else { None })
    }

    pub fn iter_hiders(&self) -> impl Iterator<Item = PlayerId> + use<'_> {
        self.caught_state
            .iter()
            .filter_map(|(k, v)| if !*v { Some(*k) } else { None })
    }
}

impl PlayerState {
    pub fn new(id: PlayerId, seeker: bool) -> Self {
        Self {
            id,
            locations: Vec::with_capacity(20),
            seeker,
            held_powerup: None,
        }
    }

    /// Create a [PlayerPing] with the latest location saved for the player
    pub fn create_self_ping(&self) -> Option<PlayerPing> {
        self.create_ping(self.id)
    }

    /// Create a [PlayerPing] with the latest location as another player, used when powerups are
    /// active
    pub fn create_ping(&self, id: PlayerId) -> Option<PlayerPing> {
        self.get_loc()
            .map(|loc| PlayerPing::new(loc.clone(), id, self.id))
    }

    /// Push a new player location
    pub fn push_loc(&mut self, loc: Location) {
        self.locations.push(loc);
    }

    /// Get the latest player location
    pub fn get_loc(&self) -> Option<&Location> {
        self.locations.last()
    }
}

/// Central struct for managing the entire game's state
pub struct GameState<L: LocationService> {
    /// Player state, different for each player
    pub player: PlayerState,
    /// Public state, kept in sync via events
    pub public: PublicState,
    /// Host state, only for host, this being [Option::None] implies not being host
    pub host: Option<HostState>,
    /// The settings for the current game, read only
    pub settings: GameSettings,
    loc: L,
}

#[derive(Clone)]
/// Enum representing all events that can be published, some are host only although
/// implicit trust is given to all players because uh who cares.
pub enum GameEvent {
    /// Seekers are now active and can see the map
    SeekersReleased(DT),
    /// (Host) A request for a given player to ping, optionally includes another player to ping
    /// *as* (e.g. when [PowerUpType::PingSeeker] is used)
    PingReq(Option<PlayerId>),
    /// A [PlayerPing] was published to [PublicState]
    Ping(PlayerPing),
    /// The given hider has been caught
    HiderCaught(PlayerId),
    /// (Host) The powerup has spawned and is available to grab
    PowerUpSpawn(PowerUp),
    /// The powerup has despawned (Was grabbed or timed out)
    PowerUpDespawn,
    /// A player has activated a powerup, some powerups will be published globally and some will be
    /// handled only by the host (such as [PowerUpType::PingSeeker])
    PowerUpActivate(PowerUpUsage),
    /// (Host) The game has ended (all players were caught or the host cancelled the game)
    GameEnd(DT),
    /// (Players) After the game has ended, players send this as the final game message
    /// to the host with their entire location history
    PostGameSync(Vec<Location>),
    /// (Host) After the game has ended and all players have sent their location histories to the
    /// host, the host will send this back to all players. Contains the entire history of the game
    /// to be saved and replayed.
    HostHistorySync(
        (
            HashMap<PlayerId, Vec<Location>>,
            Vec<(PlayerId, DT, GameEvent)>,
        ),
    ),
}

impl<L: LocationService> GameState<L> {
    /// Create a new game state (starting a game). Needs the ID of the current player and a HashMap
    /// of other player ids to their caught state (whether they start out as seeker).
    pub fn new(
        host: bool,
        id: PlayerId,
        players: HashMap<PlayerId, bool>,
        settings: GameSettings,
        loc: L,
    ) -> Self {
        let is_seeker = players.get(&id).copied().unwrap_or_default();
        Self {
            player: PlayerState::new(id, is_seeker),
            public: PublicState::new(players),
            host: if host { Some(HostState::new()) } else { None },
            settings,
            loc,
        }
    }

    pub fn random_other_hider(&mut self) -> Option<PlayerId> {
        let hiders = self
            .public
            .iter_hiders()
            .filter(|i| *i != self.player.id)
            .collect::<Vec<_>>();
        let choice = rand::random_range(0..hiders.len());
        hiders.get(choice).copied()
    }

    fn host_tick(&mut self, events: &mut Vec<(Option<PlayerId>, GameEvent)>) {
        if let Some(host) = self.host.as_mut() {
            let now = Utc::now();

            // Do seekers need to be released?
            if self.public.seekers_started.is_none()
                && (now - self.public.game_started)
                    .num_seconds()
                    .unsigned_abs()
                    >= self.settings.hiding_time_seconds
            {
                events.push((None, GameEvent::SeekersReleased(now)));
            }

            // Do we need to start doing global pings?
            if host.last_ping.is_none() {
                let should_start = match self.settings.ping_start {
                    PingStartCondition::Players(players) => {
                        self.public.caught_state.values().filter(|v| **v).count()
                            >= (players as usize)
                    }
                    PingStartCondition::Minutes(min) => {
                        let delta = now - self.public.game_started;
                        delta.num_minutes() >= (min as i64)
                    }
                    PingStartCondition::Instant => true,
                };
                if should_start {
                    host.last_ping = Some(now);
                }
            }

            // Do we need to do a global ping?
            if let Some(last_ping) = host.last_ping.as_mut() {
                if (now - *last_ping).num_minutes().unsigned_abs()
                    >= self.settings.ping_minutes_interval
                {
                    events.extend(self.public.caught_state.iter().filter_map(
                        |(player, caught)| {
                            // If caught, don't send a ping request
                            if *caught {
                                None
                            } else {
                                // If the player is pinging as someone else, do that here.
                                if let Some(PowerUpUsage::PingSeeker) =
                                    host.ping_power_usages.get(player)
                                {
                                    host.ping_power_usages.remove(player);
                                    let seekers = self.public.iter_seekers().collect::<Vec<_>>();
                                    let choice = rand::random_range(0..seekers.len());
                                    let seeker = seekers[choice];
                                    return Some((Some(seeker), GameEvent::PingReq(Some(*player))));
                                }
                                Some((Some(*player), GameEvent::PingReq(None)))
                            }
                        },
                    ));

                    *last_ping = now;
                }
            }

            // Do we need to start rolling for powerups?
            if host.last_powerup_proc.is_none() {
                let should_start = match self.settings.ping_start {
                    PingStartCondition::Players(players) => {
                        self.public.caught_state.values().filter(|v| **v).count()
                            >= (players as usize)
                    }
                    PingStartCondition::Minutes(min) => {
                        let delta = now - self.public.game_started;
                        delta.num_minutes() >= (min as i64)
                    }
                    PingStartCondition::Instant => true,
                };
                if should_start {
                    host.last_powerup_proc = Some(now);
                }
            }

            // Should we roll for a powerup?
            if let Some(last_powerup_proc) = host.last_powerup_proc.as_mut() {
                if (now - *last_powerup_proc).num_minutes() >= 1 {
                    // A minute has passed, roll to see if we should spawn a powerup
                    let cooldown_over = host.last_powerup.is_none_or(|d| {
                        (now - d).num_minutes() >= (self.settings.powerup_minutes_cooldown as i64)
                    });
                    let roll = rand::random_ratio(self.settings.powerup_chance, 100);

                    if cooldown_over && roll {
                        // Cooldown is over and we rolled positive, choose and send out a powerup.
                        let typ_choice = rand::random_range(0..PowerUpType::ALL_TYPES.len());
                        let loc_choice =
                            rand::random_range(0..self.settings.powerup_locations.len());
                        let powerup = PowerUp::new(
                            self.settings.powerup_locations[loc_choice],
                            PowerUpType::ALL_TYPES[typ_choice],
                        );

                        events.push((None, GameEvent::PowerUpSpawn(powerup)));

                        host.last_powerup = Some(now);
                    }

                    *last_powerup_proc = now;
                }
            }
        }
    }

    fn update_loc(&mut self) {
        let loc = self.loc.get_loc();
        self.player.push_loc(loc);
    }

    /// Run a single game tick, returns any messages that need to be sent
    pub fn tick(&mut self) -> Vec<(Option<PlayerId>, GameEvent)> {
        let mut events = Vec::with_capacity(5);

        self.host_tick(&mut events);
        self.update_loc();

        events
    }

    /// Consume an event, optionally returns events to re-broadcast
    pub fn consume_event(
        &mut self,
        time_sent: DT,
        event: GameEvent,
        player_id: PlayerId,
    ) -> Option<GameEvent> {
        if let Some(host) = self.host.as_mut() {
            host.event_history
                .push((player_id, time_sent, event.clone()));
        }

        match event {
            GameEvent::SeekersReleased(time) => {
                self.public.seekers_started = Some(time);
            }
            GameEvent::PingReq(fake_player) => {
                let ping = if let Some(fake_player) = fake_player {
                    self.player.create_ping(fake_player)
                } else {
                    self.player.create_self_ping()
                };

                return ping.map(|p| GameEvent::Ping(p));
            }
            GameEvent::Ping(ping) => {
                if let Some(current) = self.public.pings.get_mut(&ping.player) {
                    *current = Some(ping);
                }
            }
            GameEvent::HiderCaught(id) => {
                if id == self.player.id {
                    self.player.seeker = true;
                }
                if let Some(state) = self.public.caught_state.get_mut(&id) {
                    *state = true;
                }
                if self.host.is_some() && self.public.caught_state.iter().all(|(_, k)| *k) {
                    return Some(GameEvent::GameEnd(Utc::now()));
                }
            }
            GameEvent::GameEnd(_dt) => {
                // [Game] handles this case, do nothing if we get here.
            }
            GameEvent::PowerUpSpawn(power_up) => {
                self.public.available_powerup = Some(power_up);
            }
            GameEvent::PowerUpDespawn => {
                self.public.available_powerup = None;
            }
            GameEvent::PowerUpActivate(usage) => {
                if usage.timing() == PowerUpTiming::NextPing {
                    if let Some(host) = self.host.as_mut() {
                        if let Some(old_usage) = host.ping_power_usages.get_mut(&player_id) {
                            *old_usage = usage;
                        } else {
                            host.ping_power_usages.insert(player_id, usage);
                        }
                    }
                }
            }
            GameEvent::PostGameSync(_) | GameEvent::HostHistorySync(_) => {
                // Handled by [Game]
            }
        }
        None
    }
}

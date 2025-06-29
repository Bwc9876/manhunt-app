use anyhow::bail;
use chrono::{DateTime, Utc};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

use tokio::{sync::RwLock, time::MissedTickBehavior};

use crate::StartGameInfo;
use crate::{prelude::*, transport::TransportMessage};

use crate::{
    game_events::GameEvent,
    game_state::{GameHistory, GameState, GameUiState},
    location::LocationService,
    powerups::PowerUpType,
    settings::GameSettings,
    transport::Transport,
};

pub type Id = Uuid;

/// Convenience alias for UTC DT
pub type UtcDT = DateTime<Utc>;

pub trait StateUpdateSender {
    fn send_update(&self);
}

/// Struct representing an ongoing game, handles communication with
/// other clients via [Transport], gets location with [LocationService], and provides high-level methods for
/// taking actions in the game.
pub struct Game<L: LocationService, T: Transport, S: StateUpdateSender> {
    state: RwLock<GameState>,
    transport: Arc<T>,
    location: L,
    state_update_sender: S,
    interval: Duration,
}

impl<L: LocationService, T: Transport, S: StateUpdateSender> Game<L, T, S> {
    pub fn new(
        interval: Duration,
        start_info: StartGameInfo,
        transport: Arc<T>,
        location: L,
        state_update_sender: S,
    ) -> Self {
        let state = GameState::new(
            start_info.settings,
            transport.self_id(),
            start_info.initial_caught_state,
        );

        Self {
            transport,
            location,
            interval,
            state: RwLock::new(state),
            state_update_sender,
        }
    }

    async fn send_event(&self, event: GameEvent) {
        self.transport.send_message(event.into()).await;
    }

    pub async fn mark_caught(&self) {
        let mut state = self.state.write().await;
        let id = state.id;
        state.mark_caught(id);
        state.remove_ping(id);
        // TODO: Maybe reroll for new powerups (specifically seeker ones) instead of just erasing it
        state.use_powerup();

        self.send_event(GameEvent::PlayerCaught(state.id)).await;
    }

    pub async fn clone_settings(&self) -> GameSettings {
        self.state.read().await.clone_settings()
    }

    pub async fn get_ui_state(&self) -> GameUiState {
        self.state.read().await.as_ui_state()
    }

    pub async fn get_powerup(&self) {
        let mut state = self.state.write().await;
        state.get_powerup();
        self.send_event(GameEvent::PowerupDespawn(state.id)).await;
    }

    pub async fn use_powerup(&self) {
        let mut state = self.state.write().await;

        if let Some(powerup) = state.use_powerup() {
            match powerup {
                PowerUpType::PingSeeker => {}
                PowerUpType::PingAllSeekers => {
                    for seeker in state.iter_seekers() {
                        self.send_event(GameEvent::ForcePing(seeker, None)).await;
                    }
                }
                PowerUpType::ForcePingOther => {
                    // Fallback to a seeker if there are no other hiders
                    let target = state.random_other_hider().or_else(|| state.random_seeker());

                    if let Some(target) = target {
                        self.send_event(GameEvent::ForcePing(target, None)).await;
                    }
                }
            }
        }
    }

    async fn consume_event(&self, state: &mut GameState, event: GameEvent) {
        if !state.game_ended() {
            state.event_history.push((Utc::now(), event.clone()));
        }

        match event {
            GameEvent::Ping(player_ping) => state.add_ping(player_ping),
            GameEvent::ForcePing(target, display) => {
                if target != state.id {
                    return;
                }

                let ping = if let Some(display) = display {
                    state.create_ping(display)
                } else {
                    state.create_self_ping()
                };

                if let Some(ping) = ping {
                    state.add_ping(ping.clone());
                    self.send_event(GameEvent::Ping(ping)).await;
                }
            }
            GameEvent::PowerupDespawn(_) => state.despawn_powerup(),
            GameEvent::PlayerCaught(player) => {
                state.mark_caught(player);
                state.remove_ping(player);
            }
            GameEvent::PostGameSync(id, history) => {
                state.insert_player_location_history(id, history);
            }
        }

        self.state_update_sender.send_update();
    }

    async fn consume_message(
        &self,
        state: &mut GameState,
        _id: Option<Uuid>,
        msg: TransportMessage,
    ) -> Result<bool> {
        match msg {
            TransportMessage::Game(event) => {
                self.consume_event(state, *event).await;
                Ok(false)
            }
            TransportMessage::PeerDisconnect(id) => {
                state.remove_player(id);
                Ok(false)
            }
            TransportMessage::Disconnected => {
                // Expected disconnect, exit
                Ok(true)
            }
            TransportMessage::Error(err) => bail!("Transport error: {err}"),
            _ => Ok(false),
        }
    }

    /// Perform a tick for a specific moment in time
    /// Returns whether the game loop should be broken.
    async fn tick(&self, state: &mut GameState, now: UtcDT) -> bool {
        let mut send_update = false;

        if state.check_end_game() {
            // If we're at the point where the game is over, send out our location history
            let msg = GameEvent::PostGameSync(state.id, state.location_history.clone());
            self.send_event(msg).await;
            send_update = true;
        }

        if state.game_ended() {
            // Don't do normal ticks if the game is over,
            // simply return if we're done doing a post-game sync
            if send_update {
                self.state_update_sender.send_update();
            }
            return state.check_post_game_sync();
        }

        // Push to location history
        if let Some(location) = self.location.get_loc() {
            state.push_loc(location);
        }

        // Release Seekers?
        if !state.seekers_released() && state.should_release_seekers(now) {
            state.release_seekers(now);
            send_update = true;
        }

        // Start Pings?
        if !state.pings_started() && state.should_start_pings(now) {
            state.start_pings(now);
            send_update = true;
        }

        // Do a Ping?
        if state.should_ping(&now) {
            if let Some(&PowerUpType::PingSeeker) = state.peek_powerup() {
                // We have a powerup that lets us ping a seeker as us, use it.
                if let Some(seeker) = state.random_seeker() {
                    state.use_powerup();
                    self.send_event(GameEvent::ForcePing(seeker, Some(state.id)))
                        .await;
                    state.start_pings(now);
                }
            } else {
                // No powerup, normal ping
                if let Some(ping) = state.create_self_ping() {
                    self.send_event(GameEvent::Ping(ping.clone())).await;
                    state.add_ping(ping);
                    state.start_pings(now);
                }
            }
        }

        // Start Powerup Rolls?
        if !state.powerups_started() && state.should_start_powerups(now) {
            state.start_powerups(now);
            send_update = true;
        }

        // Should roll for a powerup?
        if state.should_spawn_powerup(&now) {
            state.try_spawn_powerup(now);
            send_update = true;
        }

        // Send a state update to the UI?
        if send_update {
            self.state_update_sender.send_update();
        }

        false
    }

    #[cfg(test)]
    pub async fn force_tick(&self, now: UtcDT) {
        let mut state = self.state.write().await;
        self.tick(&mut state, now).await;
    }

    pub async fn quit_game(&self) {
        self.transport.disconnect().await;
    }

    /// Main loop of the game, handles ticking and receiving messages from [Transport].
    pub async fn main_loop(&self) -> Result<Option<GameHistory>> {
        let mut interval = tokio::time::interval(self.interval);

        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let res = 'game: loop {
            tokio::select! {
                biased;

                messages = self.transport.receive_messages() => {
                    let mut state = self.state.write().await;
                    for (id, msg) in messages {
                        match self.consume_message(&mut state, id, msg).await {
                            Ok(should_break) => {
                                if should_break {
                                    break 'game Ok(None);
                                }
                            }
                            Err(why) => { break 'game Err(why); }
                        }
                    }
                }

                _ = interval.tick() => {
                    let mut state = self.state.write().await;
                    let should_break = self.tick(&mut state, Utc::now()).await;

                    if should_break {
                        let history = state.as_game_history();
                        break Ok(Some(history));
                    }
                }
            }
        };

        self.transport.disconnect().await;

        res
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use crate::{
        location::Location,
        settings::PingStartCondition,
        tests::{DummySender, MockLocation, MockTransport},
    };

    use super::*;
    use tokio::{task::yield_now, test};

    type TestGame = Game<MockLocation, MockTransport, DummySender>;

    struct MockMatch {
        uuids: Vec<Uuid>,
        games: HashMap<u32, Arc<TestGame>>,
        settings: GameSettings,
        mock_now: UtcDT,
    }

    const INTERVAL: Duration = Duration::from_secs(u64::MAX);

    impl MockMatch {
        pub fn new(settings: GameSettings, players: u32, seekers: u32) -> Self {
            let (uuids, transports) = MockTransport::create_mesh(players);

            let initial_caught_state = (0..players)
                .map(|id| (uuids[id as usize], id < seekers))
                .collect::<HashMap<_, _>>();

            let games = transports
                .into_iter()
                .enumerate()
                .map(|(id, transport)| {
                    let location = MockLocation;
                    let start_info = StartGameInfo {
                        initial_caught_state: initial_caught_state.clone(),
                        settings: settings.clone(),
                    };
                    let game = TestGame::new(
                        INTERVAL,
                        start_info,
                        Arc::new(transport),
                        location,
                        DummySender,
                    );

                    (id as u32, Arc::new(game))
                })
                .collect::<HashMap<_, _>>();

            Self {
                settings,
                games,
                uuids,
                mock_now: Utc::now(),
            }
        }

        pub async fn start(&self) {
            for game in self.games.values() {
                let game = game.clone();
                tokio::spawn(async move {
                    game.main_loop().await.expect("Game Start Fail");
                });
                yield_now().await;
            }
        }

        pub async fn pass_time(&mut self, d: Duration) {
            self.mock_now += d;
        }

        pub async fn assert_all_states(&self, f: impl Fn(&GameState)) {
            for game in self.games.values() {
                let state = game.state.read().await;
                f(&state);
            }
        }

        pub fn game(&self, id: u32) -> &TestGame {
            self.games.get(&id).as_ref().unwrap()
        }

        pub async fn wait_for_seekers(&mut self) {
            let hiding_time = Duration::from_secs(self.settings.hiding_time_seconds as u64 + 1);
            self.mock_now += hiding_time;

            self.tick().await;

            self.assert_all_states(|s| {
                assert!(s.seekers_released());
            })
            .await;
        }

        async fn tick_all(&self, now: UtcDT) {
            for game in self.games.values() {
                game.force_tick(now).await;
            }
        }

        pub async fn tick(&self) {
            self.tick_all(self.mock_now).await;
            yield_now().await;
        }
    }

    fn mk_settings() -> GameSettings {
        GameSettings {
            random_seed: 0,
            hiding_time_seconds: 1,
            ping_start: PingStartCondition::Instant,
            ping_minutes_interval: 1,
            powerup_start: PingStartCondition::Instant,
            powerup_chance: 0,
            powerup_minutes_cooldown: 1,
            powerup_locations: vec![Location {
                lat: 0.0,
                long: 0.0,
                heading: None,
            }],
        }
    }

    #[test]
    async fn test_minimal_game() {
        let settings = mk_settings();

        // 2 players, one is a seeker
        let mut mat = MockMatch::new(settings, 2, 1);

        mat.start().await;

        mat.wait_for_seekers().await;

        mat.game(1).mark_caught().await;

        mat.tick().await;

        mat.assert_all_states(|s| {
            assert_eq!(
                s.get_caught(mat.uuids[1]),
                Some(true),
                "Game {} sees player 1 as not caught",
                s.id
            );
        })
        .await;

        // Extra tick for post-game syncing
        mat.tick().await;

        mat.assert_all_states(|s| assert!(s.game_ended(), "Game {} has not ended", s.id))
            .await;
    }

    #[test]
    async fn test_basic_pinging() {
        let mut settings = mk_settings();
        settings.ping_minutes_interval = 0;

        let mut mat = MockMatch::new(settings, 4, 1);

        mat.start().await;

        mat.wait_for_seekers().await;

        mat.assert_all_states(|s| {
            for id in 0..4 {
                let ping = s.get_ping(mat.uuids[id]);
                if id == 0 {
                    assert!(
                        ping.is_none(),
                        "Game 0 is a seeker and shouldn't be pinged (in {})",
                        s.id
                    );
                } else {
                    assert!(
                        ping.is_some(),
                        "Game {} is a hider and should be pinged (in {})",
                        id,
                        s.id
                    );
                }
            }
        })
        .await;

        mat.game(1).mark_caught().await;

        mat.tick().await;

        mat.assert_all_states(|s| {
            for id in 0..4 {
                let ping = s.get_ping(mat.uuids[id]);
                if id <= 1 {
                    assert!(
                        ping.is_none(),
                        "Game {} is a seeker and shouldn't be pinged (in {})",
                        id,
                        s.id
                    );
                } else {
                    assert!(
                        ping.is_some(),
                        "Game {} is a hider and should be pinged (in {})",
                        id,
                        s.id
                    );
                }
            }
        })
        .await;
    }

    #[test]
    async fn test_rng_sync() {
        let mut settings = mk_settings();
        settings.powerup_chance = 100;
        settings.powerup_minutes_cooldown = 1;
        settings.powerup_start = PingStartCondition::Instant;
        settings.powerup_locations = (1..1000)
            .map(|x| Location {
                lat: x as f64,
                long: 1.0,
                heading: None,
            })
            .collect();

        let mut mat = MockMatch::new(settings, 10, 2);

        mat.start().await;
        mat.tick().await;
        mat.wait_for_seekers().await;
        mat.pass_time(Duration::from_secs(60)).await;
        mat.tick().await;

        let game = mat.game(0);
        let state = game.state.read().await;
        let location = state.powerup_location().expect("Powerup didn't spawn");

        drop(state);

        mat.assert_all_states(|s| {
            assert_eq!(
                s.powerup_location(),
                Some(location),
                "Game {} has a different location than 0",
                s.id
            );
        })
        .await;
    }

    #[test]
    async fn test_powerup_ping_seeker_as_you() {
        let mut settings = mk_settings();
        settings.ping_minutes_interval = 0;
        let mut mat = MockMatch::new(settings, 2, 1);

        mat.start().await;
        mat.wait_for_seekers().await;

        let game = mat.game(1);
        let mut state = game.state.write().await;
        state.force_set_powerup(PowerUpType::PingSeeker);
        drop(state);

        mat.tick().await;

        mat.assert_all_states(|s| {
            if let Some(ping) = s.get_ping(mat.uuids[1]) {
                assert_eq!(
                    ping.real_player, mat.uuids[0],
                    "Ping for 1 is not truly 0 (in {})",
                    s.id
                );
            } else {
                panic!("No ping for 1 (in {})", s.id);
            }
        })
        .await;
    }

    #[test]
    async fn test_powerup_ping_random_hider() {
        let settings = mk_settings();

        let mut mat = MockMatch::new(settings, 3, 1);

        mat.start().await;
        mat.wait_for_seekers().await;

        let game = mat.game(1);
        let mut state = game.state.write().await;
        state.force_set_powerup(PowerUpType::ForcePingOther);
        drop(state);

        game.use_powerup().await;
        mat.tick().await;

        mat.assert_all_states(|s| {
            // Player 0 is a seeker, player 1 user the powerup, so 2 is the only one that should
            // could have pinged
            assert!(s.get_ping(mat.uuids[2]).is_some());
            assert!(s.get_ping(mat.uuids[0]).is_none());
            assert!(s.get_ping(mat.uuids[1]).is_none());
        })
        .await;
    }

    #[test]
    async fn test_powerup_ping_seekers() {
        let settings = mk_settings();

        let mat = MockMatch::new(settings, 5, 3);

        mat.start().await;

        let game = mat.game(3);
        let mut state = game.state.write().await;
        state.force_set_powerup(PowerUpType::PingAllSeekers);
        drop(state);

        game.use_powerup().await;
        mat.tick().await;

        mat.assert_all_states(|s| {
            for id in 0..3 {
                assert!(
                    s.get_caught(mat.uuids[id]).is_some(),
                    "Player {} should be pinged due to the powerup (in {})",
                    id,
                    s.id
                );
            }
        })
        .await;
    }

    #[test]
    async fn test_player_dropped() {
        let settings = mk_settings();
        let mat = MockMatch::new(settings, 4, 1);

        mat.start().await;

        let game = mat.game(2);
        game.quit_game().await;
        let id = game.state.read().await.id;

        mat.tick().await;

        mat.assert_all_states(|s| {
            if s.id != id {
                assert!(
                    s.get_ping(id).is_none(),
                    "Game {} has not removed 2 from pings",
                    s.id
                );
                assert!(
                    s.get_caught(id).is_none(),
                    "Game {} has not removed 2 from caught state",
                    s.id
                );
            }
        })
        .await;
    }
}

use anyhow::bail;
use chrono::{DateTime, Utc};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use tokio::sync::{RwLock, RwLockWriteGuard};

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
    cancel: CancellationToken,
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
            cancel: CancellationToken::new(),
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
        drop(state);
        self.send_event(GameEvent::PlayerCaught(id)).await;
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

    pub async fn quit_game(&self) {
        self.cancel.cancel();
    }

    #[cfg(test)]
    fn get_now() -> UtcDT {
        let fake = tokio::time::Instant::now();
        let real = std::time::Instant::now();
        Utc::now() + (fake.into_std().duration_since(real) + Duration::from_secs(1))
    }

    #[cfg(not(test))]
    fn get_now() -> UtcDT {
        Utc::now()
    }

    /// Main loop of the game, handles ticking and receiving messages from [Transport].
    pub async fn main_loop(&self) -> Result<Option<GameHistory>> {
        let mut interval = tokio::time::interval(self.interval);

        // interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let res = 'game: loop {
            tokio::select! {
                biased;

                _ = self.cancel.cancelled() => {
                    break 'game Ok(None);
                }

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
                    let should_break = self.tick(&mut state, Self::get_now()).await;

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

    pub async fn lock_state(&self) -> RwLockWriteGuard<'_, GameState> {
        self.state.write().await
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
    use tokio::{sync::oneshot, task::yield_now, test};

    type TestGame = Game<MockLocation, MockTransport, DummySender>;

    type EndRecv = oneshot::Receiver<Result<Option<GameHistory>>>;

    struct MockMatch {
        uuids: Vec<Uuid>,
        games: Vec<Arc<TestGame>>,
        settings: GameSettings,
    }

    const INTERVAL: Duration = Duration::from_secs(600000);

    impl MockMatch {
        pub fn new(settings: GameSettings, players: u32, seekers: u32) -> Self {
            tokio::time::pause();
            let (uuids, transports) = MockTransport::create_mesh(players);

            let initial_caught_state = (0..players)
                .map(|id| (uuids[id as usize], id < seekers))
                .collect::<HashMap<_, _>>();

            let games = transports
                .into_iter()
                .map(|transport| {
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

                    Arc::new(game)
                })
                .collect();

            Self {
                settings,
                games,
                uuids,
            }
        }

        pub async fn start(&self) -> Vec<EndRecv> {
            let mut recvs = Vec::with_capacity(self.games.len());
            for game in self.games.iter() {
                let game = game.clone();
                let (send, recv) = oneshot::channel();
                recvs.push(recv);
                tokio::spawn(async move {
                    let res = game.main_loop().await;
                    send.send(res).expect("Failed to send");
                });
                yield_now().await;
            }
            recvs
        }

        pub async fn assert_all_states(&self, f: impl Fn(usize, &GameState)) {
            for (i, game) in self.games.iter().enumerate() {
                let state = game.state.read().await;
                f(i, &state);
            }
        }

        pub fn assert_all_transports_disconnected(&self) {
            for game in self.games.iter() {
                assert!(
                    game.transport.is_disconnected(),
                    "Game {} is still connected",
                    game.transport.self_id()
                );
            }
        }

        pub async fn wait_for_seekers(&mut self) {
            let hiding_time = Duration::from_secs(self.settings.hiding_time_seconds as u64 + 1);

            tokio::time::sleep(hiding_time).await;

            self.tick().await;

            self.assert_all_states(|i, s| {
                assert!(s.seekers_released(), "Seekers not released on game {i}");
            })
            .await;
        }

        pub async fn wait_for_transports(&self) {
            for game in self.games.iter() {
                game.transport.wait_for_queue_empty().await;
            }
        }

        pub async fn tick(&self) {
            tokio::time::sleep(INTERVAL + Duration::from_secs(1)).await;
            self.wait_for_transports().await;
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

        let recvs = mat.start().await;

        mat.wait_for_seekers().await;

        mat.games[1].mark_caught().await;

        mat.wait_for_transports().await;

        mat.assert_all_states(|i, s| {
            assert_eq!(
                s.get_caught(mat.uuids[1]),
                Some(true),
                "Game {i} sees player 1 as not caught",
            );
        })
        .await;

        // Tick to process game end
        mat.tick().await;

        mat.assert_all_states(|i, s| {
            assert!(s.game_ended(), "Game {i} has not ended");
        })
        .await;

        // Tick for post-game sync
        mat.tick().await;

        mat.assert_all_transports_disconnected();

        for (i, recv) in recvs.into_iter().enumerate() {
            let res = recv.await.expect("Failed to recv");
            match res {
                Ok(Some(hist)) => {
                    assert!(!hist.locations.is_empty(), "Game {i} has no locations");
                    assert!(!hist.events.is_empty(), "Game {i} has no event");
                }
                Ok(None) => {
                    panic!("Game {i} exited without a history (did not end via post game sync)");
                }
                Err(why) => {
                    panic!("Game {i} encountered error: {why:?}");
                }
            }
        }
    }

    #[test]
    async fn test_basic_pinging() {
        let mut settings = mk_settings();
        settings.ping_minutes_interval = 0;

        let mut mat = MockMatch::new(settings, 4, 1);

        mat.start().await;

        mat.wait_for_seekers().await;

        mat.assert_all_states(|i, s| {
            for id in 0..4 {
                let ping = s.get_ping(mat.uuids[id]);
                if id == 0 {
                    assert!(
                        ping.is_none(),
                        "Game {i} has a ping for 0, despite them being a seeker",
                    );
                } else {
                    assert!(
                        ping.is_some(),
                        "Game {i} doesn't have a ping for {id}, despite them being a hider",
                    );
                }
            }
        })
        .await;

        mat.games[1].mark_caught().await;

        mat.tick().await;

        mat.assert_all_states(|i, s| {
            for id in 0..4 {
                let ping = s.get_ping(mat.uuids[id]);
                if id <= 1 {
                    assert!(
                        ping.is_none(),
                        "Game {i} has a ping for {id}, despite them being a seeker",
                    );
                } else {
                    assert!(
                        ping.is_some(),
                        "Game {i} doesn't have a ping for {id}, despite them being a hider",
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
        tokio::time::sleep(Duration::from_secs(60)).await;
        mat.tick().await;

        let game = mat.games[0].clone();
        let state = game.state.read().await;
        let location = state.powerup_location().expect("Powerup didn't spawn");

        drop(state);

        mat.assert_all_states(|i, s| {
            assert_eq!(
                s.powerup_location(),
                Some(location),
                "Game {i} has a different location than 0",
            );
        })
        .await;
    }

    #[test]
    async fn test_powerup_ping_seeker_as_you() {
        let mut settings = mk_settings();
        settings.ping_minutes_interval = 1;
        let mut mat = MockMatch::new(settings, 2, 1);

        mat.start().await;
        mat.wait_for_seekers().await;

        mat.tick().await;

        tokio::time::sleep(Duration::from_secs(60)).await;

        let game = mat.games[1].clone();
        let mut state = game.state.write().await;
        state.force_set_powerup(PowerUpType::PingSeeker);
        drop(state);

        mat.tick().await;

        mat.assert_all_states(|i, s| {
            if let Some(ping) = s.get_ping(mat.uuids[1]) {
                assert_eq!(
                    ping.real_player, mat.uuids[0],
                    "Game {i} has a ping for 1, but it wasn't from 0"
                );
            } else {
                panic!("Game {i} has no ping for 1");
            }
        })
        .await;
    }

    #[test]
    async fn test_powerup_ping_random_hider() {
        let mut settings = mk_settings();
        settings.ping_minutes_interval = u32::MAX;

        let mut mat = MockMatch::new(settings, 3, 1);

        mat.start().await;
        mat.wait_for_seekers().await;

        let game = mat.games[1].clone();
        let mut state = game.state.write().await;
        state.force_set_powerup(PowerUpType::ForcePingOther);
        drop(state);

        game.use_powerup().await;
        mat.tick().await;

        mat.assert_all_states(|i, s| {
            // Player 0 is a seeker, player 1 used the powerup, so 2 is the only one that should
            // have pinged
            assert!(
                s.get_ping(mat.uuids[2]).is_some(),
                "Ping 2 is not present in game {i}"
            );
            assert!(
                s.get_ping(mat.uuids[0]).is_none(),
                "Ping 0 is present in game {i}"
            );
            assert!(
                s.get_ping(mat.uuids[1]).is_none(),
                "Ping 1 is present in game {i}"
            );
        })
        .await;
    }

    #[test]
    async fn test_powerup_ping_seekers() {
        let settings = mk_settings();

        let mat = MockMatch::new(settings, 5, 3);

        mat.start().await;

        mat.tick().await;

        let game = mat.games[3].clone();
        let mut state = game.state.write().await;
        state.force_set_powerup(PowerUpType::PingAllSeekers);
        drop(state);

        game.use_powerup().await;
        // One tick to send out the ForcePing
        mat.tick().await;
        // One tick to for the seekers to reply
        mat.tick().await;

        mat.assert_all_states(|i, s| {
            for id in 0..3 {
                assert!(
                    &s.get_ping(mat.uuids[id]).is_some(),
                    "Game {i} does not have a ping for {id}, despite the powerup being active",
                );
            }
        })
        .await;
    }

    #[test]
    async fn test_player_dropped() {
        let settings = mk_settings();
        let mat = MockMatch::new(settings, 4, 1);

        let mut recvs = mat.start().await;

        let game = mat.games[2].clone();
        let id = game.state.read().await.id;

        game.quit_game().await;
        let res = recvs.swap_remove(2).await.expect("Failed to recv");
        assert!(res.is_ok_and(|o| o.is_none()), "2 did not exit cleanly");
        assert!(
            game.transport.is_disconnected(),
            "2's transport is not disconnected"
        );

        mat.tick().await;

        mat.assert_all_states(|i, s| {
            if s.id != id {
                assert!(
                    s.get_ping(id).is_none(),
                    "Game {i} has not removed 2 from pings",
                );
                assert!(
                    s.get_caught(id).is_none(),
                    "Game {i} has not removed 2 from caught state",
                );
            }
        })
        .await;
    }
}

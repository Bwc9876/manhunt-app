use std::{collections::HashMap, time::Duration};
use tauri::Runtime;
use tokio::sync::RwLock;

use crate::{
    powerup::{PowerUpType, PowerUpUsage},
    state::{GameEvent, GameState, Location, PlayerId, DT},
};

type EventMessage = (PlayerId, DT, GameEvent);

/// Struct representing an ongoing game, handles communication with
/// other clients via [Transport] and provides high-level methods for
/// taking actions in the game.
struct Game<L: LocationService, T: Transport> {
    id: PlayerId,
    is_host: bool,
    state: RwLock<GameState<L>>,
    transport: T,
    interval: Duration,
}

pub trait Transport {
    async fn receive_message(&self) -> Option<EventMessage>;
    async fn send_event_to(&self, id: PlayerId, event: GameEvent);
    async fn send_event_host(&self, event: GameEvent);
    async fn send_event_multiple(&self, ids: Vec<PlayerId>, event: GameEvent);
    async fn send_event_all(&self, event: GameEvent);
}

pub trait LocationService {
    fn get_loc(&self) -> Location;
}

impl<L: LocationService, T: Transport> Game<L, T> {
    pub fn new(id: PlayerId, game_state: GameState<L>, transport: T, interval: Duration) -> Self {
        Self {
            id,
            is_host: game_state.host.is_some(),
            state: RwLock::new(game_state),
            transport,
            interval,
        }
    }

    /// Mark yourself as caught, sends out a message to all other players
    async fn mark_caught(&self) {
        self.transport
            .send_event_all(GameEvent::HiderCaught(self.id))
            .await;
    }

    /// Get the active powerup, to be called when the user is in range of the powerup
    async fn get_powerup(&self) {
        let mut state = self.state.write().await;
        if let Some(powerup) = state.public.available_powerup.take() {
            state.player.held_powerup = Some(powerup.typ);
            drop(state);
            self.transport
                .send_event_all(GameEvent::PowerUpDespawn)
                .await;
        }
    }

    async fn use_powerup(&self) {
        let mut state = self.state.write().await;
        if let Some(powerup) = state.player.held_powerup.take() {
            drop(state);
            match powerup {
                PowerUpType::PingSeeker => {
                    let e = GameEvent::PowerUpActivate(PowerUpUsage::PingSeeker);
                    self.transport.send_event_host(e).await;
                }
                PowerUpType::PingAllSeekers => {
                    let e = GameEvent::PingReq(None);
                    let state = self.state.read().await;
                    let seekers = state.public.iter_seekers().collect::<Vec<_>>();
                    drop(state);
                    let host_log = GameEvent::PowerUpActivate(PowerUpUsage::PingAllSeekers);
                    self.transport.send_event_host(host_log).await;
                    self.transport.send_event_multiple(seekers, e).await;
                }
                PowerUpType::ForcePingOther => {
                    let e = GameEvent::PingReq(None);
                    let mut state = self.state.write().await;
                    if let Some(target) = state.random_other_hider() {
                        let host_log =
                            GameEvent::PowerUpActivate(PowerUpUsage::ForcePingOther(target));
                        self.transport.send_event_host(host_log).await;
                        self.transport.send_event_to(target, e).await;
                    }
                }
            }
        }
    }

    /// Start main loop of the game, this should ideally be put into its own thread via
    /// [tokio::spawn].
    async fn main_loop(
        &self,
    ) -> (
        HashMap<PlayerId, Vec<Location>>,
        Vec<(PlayerId, DT, GameEvent)>,
    ) {
        let interval = tokio::time::interval(self.interval);
        tokio::pin!(interval);

        let mut ended = false;

        while !ended {
            tokio::select! {
                _ = interval.tick() => {
                    let mut state = self.state.write().await;
                    let messages = state.tick();
                    drop(state);
                    for (player, event) in messages {
                        if let Some(player) = player {
                            self.transport.send_event_to(player, event).await;
                        } else {
                            self.transport.send_event_all(event).await;
                        }
                    }
                }

                Some((player, time_sent, event)) = self.transport.receive_message() => {
                    if let GameEvent::GameEnd(dt) = event {
                        ended = true;

                    } else {
                        let mut state = self.state.write().await;
                        let new_event = state.consume_event(time_sent, event, player);
                        drop(state);
                        if let Some(event) = new_event {
                            self.transport.send_event_all(event).await;
                        }
                    }
                }
            }
        }

        let state = self.state.read().await;
        let locations = state.player.locations.clone();

        if self.is_host {
            let player_count = state.public.caught_state.len();
            let mut player_location_history =
                HashMap::<PlayerId, Vec<Location>>::with_capacity(player_count);
            player_location_history.insert(self.id, locations);
            while player_location_history.len() != player_count {
                // TODO: Join with a timeout, etc
                if let Some((id, _, GameEvent::PostGameSync(player_locations))) =
                    self.transport.receive_message().await
                {
                    player_location_history.insert(id, player_locations);
                }
            }
            let history = (
                player_location_history,
                state.host.as_ref().unwrap().event_history.clone(),
            );
            let ev = GameEvent::HostHistorySync(history.clone());
            self.transport.send_event_all(ev).await;
            history
        } else {
            self.transport
                .send_event_host(GameEvent::PostGameSync(locations))
                .await;
            loop {
                if let Some((_, _, GameEvent::HostHistorySync(history))) =
                    self.transport.receive_message().await
                {
                    break history;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::state::{GameSettings, HostState, PingStartCondition};

    use super::*;
    use tokio::sync::mpsc::{Receiver, Sender};
    use tokio::sync::Mutex;
    use tokio::task::yield_now;
    use tokio::test;

    type EventRx = Receiver<EventMessage>;
    type EventTx = Sender<EventMessage>;

    struct MockTransport {
        player_id: PlayerId,
        rx: Mutex<EventRx>,
        txs: HashMap<PlayerId, EventTx>,
    }

    impl MockTransport {
        fn new(player_id: PlayerId) -> (Self, EventTx) {
            let (tx, rx) = tokio::sync::mpsc::channel(5);
            let trans = Self {
                player_id,
                rx: Mutex::new(rx),
                txs: HashMap::new(),
            };
            (trans, tx)
        }

        fn set_txs(&mut self, txs: HashMap<PlayerId, EventTx>) {
            self.txs = txs;
        }

        fn make_msg(&self, e: GameEvent) -> EventMessage {
            (self.player_id, chrono::Utc::now(), e)
        }
    }

    impl Transport for MockTransport {
        async fn receive_message(&self) -> Option<EventMessage> {
            let mut rx = self.rx.lock().await;
            rx.recv().await
        }

        async fn send_event_to(&self, id: PlayerId, event: GameEvent) {
            if let Some(tx) = self.txs.get(&id) {
                if let Err(why) = tx.send(self.make_msg(event)).await {
                    eprintln!("Error sending msg to {id}: {why}");
                }
            }
        }

        async fn send_event_host(&self, event: GameEvent) {
            // While testing, host is always player 0
            self.send_event_to(0, event).await;
        }

        async fn send_event_multiple(&self, ids: Vec<PlayerId>, event: GameEvent) {
            for id in ids {
                self.send_event_to(id, event.clone()).await;
            }
        }

        async fn send_event_all(&self, event: GameEvent) {
            for id in self.txs.keys() {
                self.send_event_to(*id, event.clone()).await;
            }
        }
    }

    struct MockLocation;

    impl LocationService for MockLocation {
        fn get_loc(&self) -> Location {
            Location {
                lat: 0.0,
                long: 0.0,
                heading: None,
            }
        }
    }

    type MockGame = Game<MockLocation, MockTransport>;

    struct TestMatch {
        games: HashMap<PlayerId, Arc<MockGame>>,
    }

    impl TestMatch {
        /// New test match
        /// player_count: number of players
        /// num_seekers: number of seekers
        /// host_seeker: whether to mark the host as a seeker
        pub fn new(
            player_count: u32,
            num_seekers: u32,
            host_seeker: bool,
            settings: GameSettings,
        ) -> Self {
            let caught_state =
                HashMap::<PlayerId, bool>::from_iter((0..player_count).into_iter().map(|id| {
                    let should_seeker =
                        (if id == 0 || host_seeker { id } else { id - 1 }) < num_seekers;
                    (id, should_seeker && (host_seeker || id != 0))
                }));

            let mut txs = HashMap::<PlayerId, EventTx>::with_capacity(player_count as usize);
            let mut games = HashMap::<PlayerId, MockGame>::with_capacity(player_count as usize);

            for id in 0..player_count {
                let (transport, tx) = MockTransport::new(id);
                let state = GameState::new(
                    id == 0,
                    id,
                    caught_state.clone(),
                    settings.clone(),
                    MockLocation,
                );
                txs.insert(id, tx);
                let game = MockGame::new(id, state, transport, Duration::from_secs(1));
                games.insert(id, game);
            }

            for game in games.values_mut() {
                game.transport.set_txs(txs.clone());
            }

            Self {
                games: games.into_iter().map(|(k, v)| (k, Arc::new(v))).collect(),
            }
        }

        pub fn start(&self) {
            for game in self.games.values() {
                let game = game.clone();
                tokio::spawn(async move { game.main_loop().await });
            }
        }

        pub fn host(&self) -> Arc<MockGame> {
            self.game(0)
        }

        pub fn game(&self, id: PlayerId) -> Arc<MockGame> {
            self.games.get(&id).unwrap().clone()
        }

        pub async fn wait_tick(&self) {
            tokio::time::sleep(Duration::from_secs(1)).await;
            yield_now().await;
        }

        pub async fn wait_assert_seekers_released(&self) {
            tokio::time::sleep(Duration::from_secs(1)).await;
            yield_now().await;

            self.assert_all_player_states(|state| {
                assert!(state.public.seekers_started.is_some());
            });
        }

        /// Assert a condition on the host state
        pub async fn assert_host_state<F: Fn(&HostState)>(&self, f: F) {
            let host = self.host();
            let state = host.state.read().await;
            f(state.host.as_ref().unwrap());
        }

        /// Assert a condition on all player states
        pub async fn assert_all_player_states<F: Fn(&GameState<MockLocation>)>(&self, f: F) {
            for game in self.games.values() {
                let state = game.state.read().await;
                f(&state);
            }
        }
    }

    const TEST_LOC: Location = Location {
        lat: 0.0,
        long: 0.0,
        heading: None,
    };

    #[test]
    async fn test_game() {
        let settings = GameSettings {
            hiding_time_seconds: 1,
            ping_start: PingStartCondition::Players(3),
            ping_minutes_interval: 0,
            powerup_start: PingStartCondition::Players(3),
            powerup_chance: 0,
            powerup_minutes_cooldown: 1,
            powerup_locations: vec![TEST_LOC.clone()],
        };

        // A test match with 5 players, player 0 (host) is a hider, players 1 and 2 are seekers.
        let test_match = TestMatch::new(5, 2, false, settings);

        let correct_caught_state = HashMap::<PlayerId, bool>::from_iter([
            (0, false),
            (1, true),
            (2, true),
            (3, false),
            (4, false),
        ]);

        // Let's make sure our initial `caught_state` is correct
        test_match
            .assert_all_player_states(|s| assert_eq!(s.public.caught_state, correct_caught_state))
            .await;

        test_match.start();

        // Wait for seekers to be released, and then assert all player states properly reflect this
        test_match.wait_assert_seekers_released().await;

        test_match.wait_tick().await;

        // After a tick, all players should have at least one location in [PlayerState::locations]
        test_match
            .assert_all_player_states(|s| assert!(!s.player.locations.is_empty()))
            .await;

        // Now, let's see if we can mark player 3 as caught
        let player_3 = test_match.game(3);
        player_3.mark_caught().await;
        yield_now().await;

        // All states should be updated to reflect this
        test_match
            .assert_all_player_states(|s| {
                assert_eq!(s.public.caught_state.get(&3).copied(), Some(true))
            })
            .await;

        test_match.wait_tick().await;

        // And now, 3 players have been caught, meaning our [PingStartCondition] has been met,
        // let's check the host state to make sure it's starting to perform pings
        test_match
            .assert_host_state(|h| assert!(h.last_ping.is_some()))
            .await;

        test_match.wait_tick().await;

        // Value represents if the [Option] should be [Option::Some]
        let correct_pings = HashMap::<u32, bool>::from_iter([
            (0, true),
            (1, false),
            (2, false),
            (3, false),
            (4, true),
        ]);

        // Now let's make sure the hiders are being pinged (3 was just caught, triggering pings.
        // Therefore, 3 should not be pinged)
        test_match
            .assert_all_player_states(|s| {
                for (k, v) in s.public.pings.iter() {
                    assert_eq!(v.is_some(), correct_pings[k]);
                }
            })
            .await;
    }
}

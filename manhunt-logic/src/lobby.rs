use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    game::StateUpdateSender,
    prelude::*,
    profile::PlayerProfile,
    settings::GameSettings,
    transport::{Transport, TransportMessage},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartGameInfo {
    pub settings: GameSettings,
    pub initial_caught_state: HashMap<Uuid, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LobbyMessage {
    /// Message sent on a new peer, to sync profiles
    PlayerSync(Uuid, PlayerProfile),
    /// Message sent on a new peer from the host, to sync game settings
    HostPush(GameSettings),
    /// Host signals starting the game
    StartGame(StartGameInfo),
    /// A player has switched teams
    PlayerSwitch(Uuid, bool),
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LobbyState {
    profiles: HashMap<Uuid, PlayerProfile>,
    join_code: String,
    /// True represents seeker, false hider
    teams: HashMap<Uuid, bool>,
    self_id: Uuid,
    is_host: bool,
    settings: GameSettings,
}

pub struct Lobby<T: Transport, U: StateUpdateSender> {
    is_host: bool,
    join_code: String,
    state: Mutex<LobbyState>,
    transport: Arc<T>,
    state_updates: U,
    cancel: CancellationToken,
}

impl<T: Transport, U: StateUpdateSender> Lobby<T, U> {
    pub async fn new(
        join_code: &str,
        is_host: bool,
        profile: PlayerProfile,
        settings: GameSettings,
        state_updates: U,
    ) -> Result<Arc<Self>> {
        let transport = T::initialize(join_code, is_host)
            .await
            .context("Failed to connect to lobby")?;

        let lobby = Arc::new(Self::new_with_transport(
            join_code,
            is_host,
            profile,
            settings,
            state_updates,
            transport,
        ));

        Ok(lobby)
    }

    pub fn new_with_transport(
        join_code: &str,
        is_host: bool,
        profile: PlayerProfile,
        settings: GameSettings,
        state_updates: U,
        transport: Arc<T>,
    ) -> Self {
        let self_id = transport.self_id();
        Self {
            transport,
            state_updates,
            is_host,
            cancel: CancellationToken::new(),
            join_code: join_code.to_string(),
            state: Mutex::new(LobbyState {
                teams: HashMap::from_iter([(self_id, false)]),
                join_code: join_code.to_string(),
                profiles: HashMap::from_iter([(self_id, profile)]),
                self_id,
                is_host,
                settings,
            }),
        }
    }

    fn emit_state_update(&self) {
        self.state_updates.send_update();
    }

    async fn send_transport_message(&self, id: Option<Uuid>, msg: LobbyMessage) {
        if let Some(id) = id {
            self.transport.send_message_single(id, msg.into()).await
        } else {
            self.transport.send_message(msg.into()).await
        }
    }

    async fn signaling_mark_started(&self) {
        self.transport.mark_room_started(&self.join_code).await
    }

    async fn handle_lobby(&self, msg: LobbyMessage) -> Option<StartGameInfo> {
        let mut state = self.state.lock().await;
        match msg {
            LobbyMessage::PlayerSync(peer, player_profile) => {
                state.profiles.insert(peer, player_profile);
            }
            LobbyMessage::HostPush(game_settings) => {
                state.settings = game_settings;
            }
            LobbyMessage::StartGame(start_game_info) => {
                return Some(start_game_info);
            }
            LobbyMessage::PlayerSwitch(peer, seeker) => {
                state.teams.insert(peer, seeker);
            }
        }
        None
    }

    async fn handle_message(
        &self,
        peer: Option<Uuid>,
        msg: TransportMessage,
    ) -> Option<Result<Option<StartGameInfo>>> {
        match msg {
            TransportMessage::Disconnected => Some(Ok(None)),
            TransportMessage::Error(why) => Some(Err(anyhow!("Transport error: {why}"))),
            TransportMessage::Game(game_event) => {
                eprintln!("Peer {peer:?} sent a GameEvent???: {game_event:?}");
                None
            }
            TransportMessage::Lobby(lobby_message) => self
                .handle_lobby(*lobby_message)
                .await
                .map(|start_game| Ok(Some(start_game))),
            TransportMessage::PeerConnect(peer) => {
                let state = self.state.lock().await;
                let id = state.self_id;
                let msg = LobbyMessage::PlayerSync(id, state.profiles[&id].clone());
                let msg2 = LobbyMessage::PlayerSwitch(id, state.teams[&id]);
                drop(state);
                self.send_transport_message(Some(peer), msg).await;
                self.send_transport_message(Some(peer), msg2).await;
                if self.is_host {
                    let state = self.state.lock().await;
                    let msg = LobbyMessage::HostPush(state.settings.clone());
                    drop(state);
                    self.send_transport_message(Some(peer), msg).await;
                }
                None
            }
            TransportMessage::PeerDisconnect(peer) => {
                let mut state = self.state.lock().await;
                if peer != state.self_id {
                    state.profiles.remove(&peer);
                    state.teams.remove(&peer);
                }
                None
            }
        }
    }

    pub async fn main_loop(&self) -> Result<Option<StartGameInfo>> {
        let res = 'lobby: loop {
            self.emit_state_update();

            tokio::select! {
                biased;

                msgs = self.transport.receive_messages() => {
                    for (peer, msg) in msgs {
                        if let Some(res) = self.handle_message(peer, msg).await {
                            break 'lobby res;
                        }
                    }
                }

                _ = self.cancel.cancelled() => {
                    break Ok(None);
                }
            }
        };

        if let Ok(None) | Err(_) = res {
            self.transport.disconnect().await;
        }

        res
    }

    pub fn clone_transport(&self) -> Arc<T> {
        self.transport.clone()
    }

    pub async fn clone_state(&self) -> LobbyState {
        self.state.lock().await.clone()
    }

    pub async fn clone_profiles(&self) -> HashMap<Uuid, PlayerProfile> {
        let state = self.state.lock().await;
        state.profiles.clone()
    }

    /// Set self as seeker or hider
    pub async fn switch_teams(&self, seeker: bool) {
        let mut state = self.state.lock().await;
        let id = state.self_id;
        if let Some(state_seeker) = state.teams.get_mut(&id) {
            *state_seeker = seeker;
        }
        drop(state);
        let msg = LobbyMessage::PlayerSwitch(id, seeker);
        self.send_transport_message(None, msg).await;
        self.emit_state_update();
    }

    /// (Host) Update game settings
    pub async fn update_settings(&self, new_settings: GameSettings) {
        if self.is_host {
            let mut state = self.state.lock().await;
            state.settings = new_settings.clone();
            drop(state);
            let msg = LobbyMessage::HostPush(new_settings);
            self.send_transport_message(None, msg).await;
            self.emit_state_update();
        }
    }

    /// (Host) Start the game
    pub async fn start_game(&self) {
        if self.is_host {
            let state = self.state.lock().await;
            let start_game_info = StartGameInfo {
                settings: state.settings.clone(),
                initial_caught_state: state.teams.clone(),
            };
            drop(state);
            let msg = LobbyMessage::StartGame(start_game_info);
            self.signaling_mark_started().await;
            self.transport.send_self(msg.clone().into()).await;
            self.send_transport_message(None, msg).await;
        }
    }

    pub async fn quit_lobby(&self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::{sync::oneshot, task::yield_now, test};

    use crate::tests::{DummySender, MockTransport};

    type MockLobby = Lobby<MockTransport, DummySender>;

    type CompleteRecv = oneshot::Receiver<Result<Option<StartGameInfo>>>;

    struct MockLobbyPool {
        uuids: Vec<Uuid>,
        lobbies: Vec<Arc<MockLobby>>,
    }

    impl MockLobbyPool {
        pub fn new(num_players: u32) -> Self {
            let settings = GameSettings::default();
            let (uuids, transports) = MockTransport::create_mesh(num_players);

            let lobbies = transports
                .into_iter()
                .enumerate()
                .map(|(i, transport)| {
                    let profile = PlayerProfile {
                        display_name: format!("Lobby {i} ({})", uuids[i]),
                        pfp_base64: None,
                    };

                    Arc::new(MockLobby::new_with_transport(
                        "aaa",
                        i == 0,
                        profile,
                        settings.clone(),
                        DummySender,
                        Arc::new(transport),
                    ))
                })
                .collect();

            Self { uuids, lobbies }
        }

        pub async fn wait(&self) {
            for lobby in self.lobbies.iter() {
                lobby.transport.wait_for_queue_empty().await;
            }
            yield_now().await;
        }

        pub async fn start_all_loops(&self) -> Vec<CompleteRecv> {
            let mut recv_set = Vec::with_capacity(self.lobbies.len());
            for lobby in self.lobbies.iter() {
                let lobby = lobby.clone();
                let (send, recv) = oneshot::channel();
                recv_set.push(recv);
                tokio::spawn(async move {
                    let res = lobby.main_loop().await;
                    send.send(res).ok();
                });
            }
            recv_set
        }

        pub async fn player_join(&self, i: usize) {
            self.lobbies[i].transport.fake_join().await;
        }

        pub async fn assert_state(&self, i: usize, f: impl Fn(&LobbyState)) {
            let state = self.lobbies[i].state.lock().await;
            f(&state);
        }

        pub async fn assert_all_states(&self, f: impl Fn(usize, &LobbyState)) {
            for (i, lobby) in self.lobbies.iter().enumerate() {
                let state = lobby.state.lock().await;
                f(i, &state);
            }
        }
    }

    #[test]
    async fn test_joins() {
        let mat = MockLobbyPool::new(3);

        mat.start_all_loops().await;

        mat.player_join(0).await;
        mat.player_join(1).await;

        mat.wait().await;

        for i in 0..=1 {
            for j in 0..=1 {
                mat.assert_state(i, |s| {
                    assert!(
                        s.teams.contains_key(&mat.uuids[j]),
                        "{i} doesn't have {j}'s uuid in teams"
                    );
                    assert!(
                        s.profiles.contains_key(&mat.uuids[j]),
                        "{i} doesn't have {j}'s uuid in profiles"
                    );
                })
                .await;
            }
        }

        mat.lobbies[0].switch_teams(true).await;

        mat.wait().await;

        mat.player_join(2).await;

        mat.wait().await;

        mat.assert_all_states(|i, s| {
            for j in 0..=2 {
                assert!(
                    s.teams.contains_key(&mat.uuids[j]),
                    "{i} doesn't have {j}'s uuid in teams"
                );
                assert!(
                    s.profiles.contains_key(&mat.uuids[j]),
                    "{i} doesn't have {j}'s uuid in profiles"
                );
                assert_eq!(
                    s.teams.get(&mat.uuids[0]).copied(),
                    Some(true),
                    "{i} doesn't see 0 as a seeker"
                )
            }
        })
        .await;
    }

    #[test]
    async fn test_team_switch() {
        let mat = MockLobbyPool::new(3);

        mat.start_all_loops().await;

        mat.lobbies[2].switch_teams(true).await;

        mat.wait().await;

        mat.assert_all_states(|i, s| {
            assert_eq!(
                s.teams.get(&mat.uuids[2]).copied(),
                Some(true),
                "{i} ({}) does not see 2 as a seeker",
                mat.uuids[i]
            );
        })
        .await;
    }

    #[test]
    async fn test_update_settings() {
        let mat = MockLobbyPool::new(2);

        mat.start_all_loops().await;

        let mut settings = GameSettings::default();
        const UPDATED_ID: u32 = 284829;
        settings.hiding_time_seconds = UPDATED_ID;

        mat.lobbies[0].update_settings(settings).await;

        mat.wait().await;

        mat.assert_all_states(|i, s| {
            assert_eq!(
                s.settings.hiding_time_seconds, UPDATED_ID,
                "{i} ({}) did not get updated settings",
                mat.uuids[i]
            )
        })
        .await;
    }

    #[test]
    async fn test_update_settings_not_host() {
        let mat = MockLobbyPool::new(2);

        mat.start_all_loops().await;

        let mut settings = GameSettings::default();
        let target = settings.hiding_time_seconds;
        const UPDATED_ID: u32 = 284829;
        settings.hiding_time_seconds = UPDATED_ID;

        mat.lobbies[1].update_settings(settings).await;

        mat.wait().await;

        mat.assert_all_states(|i, s| {
            assert_eq!(
                s.settings.hiding_time_seconds, target,
                "{i} ({}) updated settings despite 1 not being host",
                mat.uuids[i]
            )
        })
        .await;
    }

    #[test]
    async fn test_game_start() {
        let mat = MockLobbyPool::new(4);

        let recvs = mat.start_all_loops().await;

        for i in 0..3 {
            mat.player_join(i).await;
        }

        mat.lobbies[2].switch_teams(true).await;

        let settings = GameSettings {
            hiding_time_seconds: 45,
            ..Default::default()
        };

        mat.lobbies[0].update_settings(settings).await;

        mat.lobbies[3].quit_lobby().await;

        mat.wait().await;

        mat.lobbies[0].start_game().await;

        mat.wait().await;

        for (i, recv) in recvs.into_iter().enumerate() {
            let res = recv.await.expect("Failed to recv");
            match res {
                Ok(Some(StartGameInfo {
                    settings,
                    initial_caught_state,
                })) => {
                    assert_eq!(
                        settings.hiding_time_seconds, 45,
                        "Lobby {i} does not match pushed settings"
                    );
                    assert_eq!(
                        initial_caught_state.len(),
                        3,
                        "Lobby {i} does not have 3 entries in caught state"
                    );
                    assert_eq!(
                        initial_caught_state.get(&mat.uuids[2]).copied(),
                        Some(true),
                        "Lobby {i} does not see 2 as a seeker"
                    );
                    assert!(
                        initial_caught_state.keys().all(|id| *id != mat.uuids[3]),
                        "Lobby {i} still has a disconnected player saved in caught state"
                    );
                    let profiles = mat.lobbies[i].clone_profiles().await;
                    assert_eq!(
                        profiles.len(),
                        3,
                        "Lobby {i} does not have 3 entries in profiles"
                    );
                    assert!(
                        profiles.keys().all(|id| *id != mat.uuids[3]),
                        "Lobby {i} still has a disconnected player saved in profiles"
                    );
                }
                Ok(None) => {
                    if i != 3 {
                        panic!("Lobby {i} did not exit with start info");
                    }
                }
                Err(why) => {
                    panic!("Lobby {i} had an error: {why:?}");
                }
            }
        }
    }

    #[test]
    async fn test_drop_player() {
        let mat = MockLobbyPool::new(3);

        let mut recvs = mat.start_all_loops().await;

        mat.lobbies[1].quit_lobby().await;

        let res = recvs.swap_remove(1).await.expect("Failed to recv");
        assert!(res.is_ok_and(|o| o.is_none()), "1 did not quit gracefully");

        mat.wait().await;

        assert!(
            mat.lobbies[1].transport.is_disconnected(),
            "1 is not disconnected"
        );

        let id = mat.uuids[1];

        mat.assert_all_states(|i, s| {
            if mat.uuids[i] != id {
                assert!(
                    !s.teams.contains_key(&id),
                    "{} has not been removed 1 from teams",
                    i
                );
                assert!(
                    !s.profiles.contains_key(&id),
                    "{} has not been removed 1 from profiles",
                    i
                );
            }
        })
        .await;
    }
}

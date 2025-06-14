use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    game::GameSettings,
    profile::PlayerProfile,
    transport::{MatchboxTransport, TransportMessage},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartGameInfo {
    pub settings: GameSettings,
    pub initial_caught_state: HashMap<Uuid, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LobbyMessage {
    /// Message sent on a new peer, to sync profiles
    PlayerSync(PlayerProfile),
    /// Message sent on a new peer from the host, to sync game settings
    HostPush(GameSettings),
    /// Host signals starting the game
    StartGame(StartGameInfo),
    /// A player has switched teams
    PlayerSwitch(bool),
}

#[derive(Clone, Serialize, Deserialize, specta::Type)]
pub struct LobbyState {
    profiles: HashMap<Uuid, PlayerProfile>,
    join_code: String,
    /// True represents seeker, false hider
    teams: HashMap<Uuid, bool>,
    self_seeker: bool,
    settings: GameSettings,
}

pub struct Lobby {
    is_host: bool,
    pub self_profile: PlayerProfile,
    state: Mutex<LobbyState>,
    transport: Arc<MatchboxTransport>,
}

impl Lobby {
    pub fn new(
        ws_url_base: &str,
        join_code: &str,
        host: bool,
        profile: PlayerProfile,
        settings: GameSettings,
    ) -> Self {
        Self {
            transport: Arc::new(MatchboxTransport::new(&format!(
                "{ws_url_base}/{join_code}"
            ))),
            is_host: host,
            self_profile: profile,
            state: Mutex::new(LobbyState {
                teams: HashMap::with_capacity(5),
                join_code: join_code.to_string(),
                profiles: HashMap::with_capacity(5),
                self_seeker: false,
                settings,
            }),
        }
    }

    pub fn clone_transport(&self) -> Arc<MatchboxTransport> {
        self.transport.clone()
    }

    pub async fn clone_state(&self) -> LobbyState {
        self.state.lock().await.clone()
    }

    /// Set self as seeker or hider
    pub async fn switch_teams(&self, seeker: bool) {
        let mut state = self.state.lock().await;
        state.self_seeker = seeker;
        drop(state);
        self.transport
            .send_transport_message(
                None,
                TransportMessage::Lobby(LobbyMessage::PlayerSwitch(seeker)),
            )
            .await;
    }

    /// (Host) Update game settings
    pub async fn update_settings(&self, new_settings: GameSettings) {
        if self.is_host {
            let mut state = self.state.lock().await;
            state.settings = new_settings.clone();
            drop(state);
            let msg = TransportMessage::Lobby(LobbyMessage::HostPush(new_settings));
            self.transport.send_transport_message(None, msg).await;
        }
    }

    /// (Host) Start the game
    pub async fn start_game(&self) {
        if self.is_host {
            if let Some(my_id) = self.transport.get_my_id().await {
                let mut state = self.state.lock().await;
                let seeker = state.self_seeker;
                state.teams.insert(my_id, seeker);
                let start_game_info = StartGameInfo {
                    settings: state.settings.clone(),
                    initial_caught_state: state.teams.clone(),
                };
                let msg = TransportMessage::Lobby(LobbyMessage::StartGame(start_game_info));
                self.transport.send_transport_message(None, msg).await;
            }
        }
    }

    pub async fn open(&self) -> (Uuid, StartGameInfo) {
        let transport_inner = self.transport.clone();
        tokio::spawn(async move { transport_inner.transport_loop().await });

        loop {
            if let Some((peer, msg)) = self.transport.recv_transport_message().await {
                match msg {
                    TransportMessage::Game(game_event) => {
                        eprintln!("Peer {peer:?} sent a GameEvent: {game_event:?}");
                    }
                    TransportMessage::Lobby(lobby_message) => match lobby_message {
                        LobbyMessage::PlayerSync(player_profile) => {
                            let mut state = self.state.lock().await;
                            state.profiles.insert(peer, player_profile);
                        }
                        LobbyMessage::HostPush(game_settings) => {
                            let mut state = self.state.lock().await;
                            state.settings = game_settings;
                        }
                        LobbyMessage::StartGame(start_game_info) => {
                            break (
                                self.transport
                                    .get_my_id()
                                    .await
                                    .expect("Error getting self ID"),
                                start_game_info,
                            );
                        }
                        LobbyMessage::PlayerSwitch(seeker) => {
                            let mut state = self.state.lock().await;
                            state.teams.insert(peer, seeker);
                        }
                    },
                    TransportMessage::PeerConnect => {
                        let msg = LobbyMessage::PlayerSync(self.self_profile.clone());
                        let mut state = self.state.lock().await;
                        state.teams.insert(peer, false);
                        drop(state);
                        let msg = TransportMessage::Lobby(msg);
                        self.transport.send_transport_message(Some(peer), msg).await;
                        if self.is_host {
                            let state = self.state.lock().await;
                            let msg = LobbyMessage::HostPush(state.settings.clone());
                            drop(state);
                            let msg = TransportMessage::Lobby(msg);
                            self.transport.send_transport_message(Some(peer), msg).await;
                        }
                    }
                    TransportMessage::PeerDisconnect => {
                        let mut state = self.state.lock().await;
                        state.profiles.remove(&peer);
                        state.teams.remove(&peer);
                    }
                }
            }
        }
    }
}

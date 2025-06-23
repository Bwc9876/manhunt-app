use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
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

#[derive(Clone, Serialize, Deserialize, specta::Type)]
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
}

impl<T: Transport, U: StateUpdateSender> Lobby<T, U> {
    pub async fn new(
        join_code: &str,
        host: bool,
        profile: PlayerProfile,
        settings: GameSettings,
        state_updates: U,
    ) -> Result<Arc<Self>> {
        let transport = T::initialize(join_code, host)
            .await
            .context("Failed to connect to lobby")?;

        let self_id = transport.self_id();

        let lobby = Arc::new(Self {
            transport,
            state_updates,
            is_host: host,
            join_code: join_code.to_string(),
            state: Mutex::new(LobbyState {
                teams: HashMap::from_iter([(self_id, false)]),
                join_code: join_code.to_string(),
                profiles: HashMap::from_iter([(self_id, profile)]),
                self_id,
                is_host: host,
                settings,
            }),
        });

        Ok(lobby)
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
                let mut state = self.state.lock().await;
                state.teams.insert(peer, false);
                let id = state.self_id;
                let msg = LobbyMessage::PlayerSync(id, state.profiles[&id].clone());
                drop(state);
                self.send_transport_message(Some(peer), msg).await;
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

            let msgs = self.transport.receive_messages().await;

            for (peer, msg) in msgs {
                if let Some(res) = self.handle_message(peer, msg).await {
                    break 'lobby res;
                }
            }
        };

        if res.is_err() {
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
        self.transport.disconnect().await;
    }
}

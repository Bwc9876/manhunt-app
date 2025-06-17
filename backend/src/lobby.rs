use std::{collections::HashMap, sync::Arc, time::Duration};

use log::{error, warn};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    game::GameSettings,
    prelude::*,
    profile::PlayerProfile,
    server_url,
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
    join_code: String,
    pub self_profile: PlayerProfile,
    state: Mutex<LobbyState>,
    transport: Arc<MatchboxTransport>,
    app: AppHandle,
}

/// The lobby state has updated in some way, you're expected to call [get_lobby_state] after
/// receiving this
#[derive(Serialize, Deserialize, Clone, Debug, specta::Type, tauri_specta::Event)]
pub struct LobbyStateUpdate;

impl Lobby {
    pub fn new(
        ws_url_base: &str,
        join_code: &str,
        host: bool,
        profile: PlayerProfile,
        settings: GameSettings,
        app: AppHandle,
    ) -> Self {
        Self {
            app,
            transport: Arc::new(MatchboxTransport::new(&format!(
                "{ws_url_base}/{join_code}{}",
                if host { "?create" } else { "" }
            ))),
            is_host: host,
            self_profile: profile,
            join_code: join_code.to_string(),
            state: Mutex::new(LobbyState {
                teams: HashMap::with_capacity(5),
                join_code: join_code.to_string(),
                profiles: HashMap::with_capacity(5),
                self_seeker: false,
                settings,
            }),
        }
    }

    fn emit_state_update(&self) {
        if let Err(why) = LobbyStateUpdate.emit(&self.app) {
            error!("Error emitting Lobby state update: {why:?}");
        }
    }

    pub fn clone_transport(&self) -> Arc<MatchboxTransport> {
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
        state.self_seeker = seeker;
        drop(state);
        self.transport
            .send_transport_message(None, LobbyMessage::PlayerSwitch(seeker).into())
            .await;
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

    async fn send_transport_message(&self, id: Option<Uuid>, msg: LobbyMessage) {
        self.transport.send_transport_message(id, msg.into()).await
    }

    async fn singaling_mark_started(&self) -> Result {
        let url = format!("{}/mark_started/{}", server_url(), &self.join_code);
        let client = reqwest::Client::builder().build()?;
        client.post(url).send().await?.error_for_status()?;
        Ok(())
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
                drop(state);
                let msg = LobbyMessage::StartGame(start_game_info);
                self.send_transport_message(None, msg).await;
                if let Err(why) = self.singaling_mark_started().await {
                    warn!("Failed to tell signalling server that the match started: {why:?}");
                }
                self.emit_state_update();
            }
        }
    }

    pub fn quit_lobby(&self) {
        self.transport.cancel();
    }

    pub async fn open(&self) -> Result<(Uuid, StartGameInfo)> {
        let transport_inner = self.transport.clone();
        tokio::spawn(async move { transport_inner.transport_loop().await });

        let mut interval = tokio::time::interval(Duration::from_secs(1));

        let res = 'lobby: loop {
            self.emit_state_update();

            interval.tick().await;

            let msgs = self.transport.recv_transport_messages().await;

            for (peer, msg) in msgs {
                match msg {
                    TransportMessage::Disconnected => {
                        break 'lobby Err(anyhow!(
                            "Transport disconnected before lobby could start game"
                        ));
                    }
                    TransportMessage::Game(game_event) => {
                        eprintln!("Peer {peer:?} sent a GameEvent: {game_event:?}");
                    }
                    TransportMessage::Lobby(lobby_message) => match *lobby_message {
                        LobbyMessage::PlayerSync(player_profile) => {
                            let mut state = self.state.lock().await;
                            state.profiles.insert(peer, player_profile);
                        }
                        LobbyMessage::HostPush(game_settings) => {
                            let mut state = self.state.lock().await;
                            state.settings = game_settings;
                        }
                        LobbyMessage::StartGame(start_game_info) => {
                            break 'lobby Ok((
                                self.transport
                                    .get_my_id()
                                    .await
                                    .expect("Error getting self ID"),
                                start_game_info,
                            ));
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
                        self.send_transport_message(Some(peer), msg).await;
                        if self.is_host {
                            let state = self.state.lock().await;
                            let msg = LobbyMessage::HostPush(state.settings.clone());
                            drop(state);
                            self.send_transport_message(Some(peer), msg).await;
                        }
                    }
                    TransportMessage::PeerDisconnect => {
                        let mut state = self.state.lock().await;
                        state.profiles.remove(&peer);
                        state.teams.remove(&peer);
                    }
                    TransportMessage::Seq(_) => {}
                }
            }
        };

        if res.is_err() {
            self.transport.cancel();
        }

        res
    }
}

use std::{collections::HashMap, marker::PhantomData, sync::Arc, time::Duration};

use anyhow::Context;
use log::{error, info, warn};
use manhunt_logic::{
    Game as BaseGame, GameSettings, Lobby as BaseLobby, PlayerProfile, StartGameInfo,
    StateUpdateSender, UtcDT,
};
use manhunt_transport::{MatchboxTransport, request_room_code};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_specta::Event;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    Result,
    history::AppGameHistory,
    location::TauriLocation,
    profiles::{read_profile_from_store, write_profile_to_store},
};

/// The state of the game has changed
#[derive(Serialize, Deserialize, Clone, Default, Debug, specta::Type, tauri_specta::Event)]
pub struct GameStateUpdate;

/// The state of the lobby has changed
#[derive(Serialize, Deserialize, Clone, Default, Debug, specta::Type, tauri_specta::Event)]
pub struct LobbyStateUpdate;

pub struct TauriStateUpdateSender<E: Clone + Default + Event + Serialize>(
    AppHandle,
    PhantomData<E>,
);

impl<E: Serialize + Clone + Default + Event> TauriStateUpdateSender<E> {
    fn new(app: &AppHandle) -> Self {
        Self(app.clone(), PhantomData)
    }
}

impl<E: Serialize + Clone + Default + Event> StateUpdateSender for TauriStateUpdateSender<E> {
    fn send_update(&self) {
        if let Err(why) = E::default().emit(&self.0) {
            error!("Error sending Game state update to UI: {why:?}");
        }
    }
}

type Game = BaseGame<TauriLocation, MatchboxTransport, TauriStateUpdateSender<GameStateUpdate>>;
type Lobby = BaseLobby<MatchboxTransport, TauriStateUpdateSender<LobbyStateUpdate>>;

pub enum AppState {
    Setup,
    Menu(PlayerProfile),
    Lobby(Arc<Lobby>),
    Game(Arc<Game>, HashMap<Uuid, PlayerProfile>),
    Replay(AppGameHistory),
}

#[derive(Serialize, Deserialize, specta::Type, Debug, Clone, Eq, PartialEq)]
pub enum AppScreen {
    Setup,
    Menu,
    Lobby,
    Game,
    Replay,
}

pub type AppStateHandle = RwLock<AppState>;

const GAME_TICK_RATE: Duration = Duration::from_secs(1);

/// The app is changing screens, contains the screen it's switching to
#[derive(Serialize, Deserialize, Clone, Debug, specta::Type, tauri_specta::Event)]
pub struct ChangeScreen(AppScreen);

fn error_dialog(app: &AppHandle, msg: &str) {
    app.dialog()
        .message(msg)
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

impl AppState {
    pub async fn start_game(&mut self, app: AppHandle, start: StartGameInfo) {
        if let AppState::Lobby(lobby) = self {
            let transport = lobby.clone_transport();
            let profiles = lobby.clone_profiles().await;
            let location = TauriLocation::new(app.clone());
            let state_updates = TauriStateUpdateSender::new(&app);
            let game = Arc::new(Game::new(
                GAME_TICK_RATE,
                start,
                transport,
                location,
                state_updates,
            ));
            *self = AppState::Game(game.clone(), profiles.clone());
            Self::game_loop(app.clone(), game, profiles);
            Self::emit_screen_change(&app, AppScreen::Game);
        }
    }

    fn game_loop(app: AppHandle, game: Arc<Game>, profiles: HashMap<Uuid, PlayerProfile>) {
        tokio::spawn(async move {
            let res = game.main_loop().await;
            let state_handle = app.state::<AppStateHandle>();
            let mut state = state_handle.write().await;
            match res {
                Ok(Some(history)) => {
                    let history =
                        AppGameHistory::new(history, profiles, game.clone_settings().await);
                    if let Err(why) = history.save_history(&app) {
                        error!("Failed to save game history: {why:?}");
                        error_dialog(&app, "Failed to save the history of this game");
                    }
                    state.quit_to_menu(app.clone()).await;
                }
                Ok(None) => {
                    info!("User quit game");
                }
                Err(why) => {
                    error!("Game Error: {why:?}");
                    app.dialog()
                        .message(format!("Connection Error: {why}"))
                        .kind(MessageDialogKind::Error)
                        .show(|_| {});
                    state.quit_to_menu(app.clone()).await;
                }
            }
        });
    }

    pub fn get_menu(&self) -> Result<&PlayerProfile> {
        match self {
            AppState::Menu(player_profile) => Ok(player_profile),
            _ => Err("Not on menu screen".to_string()),
        }
    }

    pub fn get_menu_mut(&mut self) -> Result<&mut PlayerProfile> {
        match self {
            AppState::Menu(player_profile) => Ok(player_profile),
            _ => Err("Not on menu screen".to_string()),
        }
    }

    pub fn get_lobby(&self) -> Result<Arc<Lobby>> {
        if let AppState::Lobby(lobby) = self {
            Ok(lobby.clone())
        } else {
            Err("Not on lobby screen".to_string())
        }
    }

    pub fn get_game(&self) -> Result<Arc<Game>> {
        if let AppState::Game(game, _) = self {
            Ok(game.clone())
        } else {
            Err("Not on game screen".to_string())
        }
    }

    pub fn get_profiles(&self) -> Result<&HashMap<Uuid, PlayerProfile>> {
        if let AppState::Game(_, profiles) = self {
            Ok(profiles)
        } else {
            Err("Not on game screen".to_string())
        }
    }

    pub fn get_replay(&self) -> Result<AppGameHistory> {
        if let AppState::Replay(history) = self {
            Ok(history.clone())
        } else {
            Err("Not on replay screen".to_string())
        }
    }

    fn emit_screen_change(app: &AppHandle, screen: AppScreen) {
        if let Err(why) = ChangeScreen(screen).emit(app) {
            warn!("Error emitting screen change: {why:?}");
        }
    }

    pub fn complete_setup(&mut self, app: &AppHandle, profile: PlayerProfile) -> Result {
        if let AppState::Setup = self {
            write_profile_to_store(app, profile.clone());
            *self = AppState::Menu(profile);
            Self::emit_screen_change(app, AppScreen::Menu);
            Ok(())
        } else {
            Err("Must be on the Setup screen".to_string())
        }
    }

    pub fn replay_game(&mut self, app: &AppHandle, id: UtcDT) -> Result {
        if let AppState::Menu(_) = self {
            let history = AppGameHistory::get_history(app, id)
                .context("Failed to read history")
                .map_err(|e| e.to_string())?;
            *self = AppState::Replay(history);
            Self::emit_screen_change(app, AppScreen::Replay);
            Ok(())
        } else {
            Err("Not on menu screen".to_string())
        }
    }

    fn lobby_loop(app: AppHandle, lobby: Arc<Lobby>) {
        tokio::spawn(async move {
            let res = lobby.main_loop().await;
            let app_game = app.clone();
            let state_handle = app.state::<AppStateHandle>();
            let mut state = state_handle.write().await;
            match res {
                Ok(Some(start)) => {
                    info!("Starting Game");
                    state.start_game(app_game, start).await;
                }
                Ok(None) => {
                    info!("User quit lobby");
                }
                Err(why) => {
                    error!("Lobby Error: {why}");
                    error_dialog(&app_game, &format!("Error joining the lobby: {why}"));
                    state.quit_to_menu(app_game).await;
                }
            }
        });
    }

    pub async fn start_lobby(
        &mut self,
        join_code: Option<String>,
        app: AppHandle,
        settings: GameSettings,
    ) {
        if let AppState::Menu(profile) = self {
            let host = join_code.is_none();
            let room_code = if let Some(code) = join_code {
                code.to_ascii_uppercase()
            } else {
                match request_room_code().await {
                    Ok(code) => code,
                    Err(why) => {
                        error_dialog(&app, &format!("Couldn't create a lobby\n\n{why:?}"));
                        return;
                    }
                }
            };
            let state_updates = TauriStateUpdateSender::<LobbyStateUpdate>::new(&app);
            let lobby =
                Lobby::new(&room_code, host, profile.clone(), settings, state_updates).await;
            match lobby {
                Ok(lobby) => {
                    *self = AppState::Lobby(lobby.clone());
                    Self::lobby_loop(app.clone(), lobby);
                    Self::emit_screen_change(&app, AppScreen::Lobby);
                }
                Err(why) => {
                    error_dialog(
                        &app,
                        &format!("Couldn't connect you to the lobby\n\n{why:?}"),
                    );
                }
            }
        }
    }

    pub async fn quit_to_menu(&mut self, app: AppHandle) {
        let profile = match self {
            AppState::Setup => None,
            AppState::Menu(_) => {
                warn!("Already on menu!");
                return;
            }
            AppState::Lobby(lobby) => {
                lobby.quit_lobby().await;
                read_profile_from_store(&app)
            }
            AppState::Game(game, _) => {
                game.quit_game().await;
                read_profile_from_store(&app)
            }
            AppState::Replay(_) => read_profile_from_store(&app),
        };
        let screen = if let Some(profile) = profile {
            *self = AppState::Menu(profile);
            AppScreen::Menu
        } else {
            *self = AppState::Setup;
            AppScreen::Setup
        };

        Self::emit_screen_change(&app, screen);
    }
}

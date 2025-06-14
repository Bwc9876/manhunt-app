mod game;
mod lobby;
mod location;
mod profile;
mod transport;

use std::{sync::Arc, time::Duration};

use game::{Game as BaseGame, GameSettings, GameState};
use lobby::{Lobby, LobbyState, StartGameInfo};
use location::TauriLocation;
use profile::PlayerProfile;
use serde::{Deserialize, Serialize};
use specta_typescript::Typescript;
use tauri::{AppHandle, Manager, State};
use tauri_specta::collect_commands;
use tokio::sync::RwLock;
use transport::MatchboxTransport;
use uuid::Uuid;

type Game = BaseGame<TauriLocation, MatchboxTransport>;

enum AppState {
    Setup,
    Menu(PlayerProfile),
    Lobby(Arc<Lobby>),
    Game(Arc<Game>),
}

type AppStateHandle = RwLock<AppState>;

fn generate_join_code() -> String {
    // 5 character sequence of A-Z
    (0..5)
        .into_iter()
        .map(|_| (('A' as u8) + rand::random_range(0..26)) as char)
        .collect::<String>()
}

const GAME_TICK_RATE: Duration = Duration::from_secs(1);

const fn server_url() -> &'static str {
    if let Some(url) = option_env!("APP_SERVER_URL") {
        url
    } else {
        "ws://localhost:3536"
    }
}

impl AppState {
    pub fn start_game(&mut self, app: AppHandle, my_id: Uuid, start: StartGameInfo) {
        match self {
            AppState::Lobby(lobby) => {
                let transport = lobby.clone_transport();
                let location = TauriLocation::new(app.clone());
                let game = Arc::new(Game::new(
                    my_id,
                    GAME_TICK_RATE,
                    start.initial_caught_state,
                    start.settings,
                    transport,
                    location,
                ));
                *self = AppState::Game(game.clone());
                tokio::spawn(async move {
                    game.main_loop().await;
                });
            }
            _ => {}
        }
    }

    pub fn start_lobby(
        &mut self,
        join_code: Option<String>,
        app: AppHandle,
        settings: GameSettings,
    ) {
        match self {
            AppState::Menu(profile) => {
                let host = join_code.is_none();
                let room_code = join_code.unwrap_or_else(generate_join_code);
                let lobby = Arc::new(Lobby::new(
                    server_url(),
                    &room_code,
                    host,
                    profile.clone(),
                    settings,
                ));
                *self = AppState::Lobby(lobby.clone());
                tokio::spawn(async move {
                    let (my_id, start) = lobby.open().await;
                    let app_game = app.clone();
                    let state_handle = app.state::<AppStateHandle>();
                    let mut state = state_handle.write().await;
                    state.start_game(app_game, my_id, start);
                });
            }
            _ => {}
        }
    }
}

use std::result::Result as StdResult;

type Result<T = (), E = String> = StdResult<T, E>;

#[derive(Serialize, Deserialize, specta::Type, Debug, Clone)]
enum AppScreen {
    Setup,
    Menu,
    Lobby,
    Game,
}

// == GENERAL / FLOW COMMANDS ==

#[tauri::command]
#[specta::specta]
/// Get the screen the app should currently be on, returns [AppScreen]
async fn get_current_screen(state: State<'_, AppStateHandle>) -> Result<AppScreen> {
    let state = state.read().await;
    Ok(match &*state {
        AppState::Setup => AppScreen::Setup,
        AppState::Menu(_player_profile) => AppScreen::Menu,
        AppState::Lobby(_lobby) => AppScreen::Lobby,
        AppState::Game(_game) => AppScreen::Game,
    })
}

#[tauri::command]
#[specta::specta]
/// Quit a running game or leave a lobby
async fn quit_game_or_lobby(app: AppHandle, state: State<'_, AppStateHandle>) -> Result {
    let mut state = state.write().await;
    let profile = match &*state {
        AppState::Setup => Err("Invalid Screen".to_string()),
        AppState::Menu(_) => Err("Already In Menu".to_string()),
        AppState::Lobby(_) | AppState::Game(_) => Ok(PlayerProfile::load_from_store(&app)),
    }?;
    if let Some(profile) = profile {
        *state = AppState::Menu(profile);
    } else {
        *state = AppState::Setup;
    }
    Ok(())
}

// == AppState::Menu COMMANDS ==

#[tauri::command]
#[specta::specta]
/// (Screen: Menu) Update the player's profile and persist it
async fn update_profile(
    new_profile: PlayerProfile,
    app: AppHandle,
    state: State<'_, AppStateHandle>,
) -> Result {
    new_profile.write_to_store(&app);
    let mut state = state.write().await;
    if let AppState::Menu(profile) = &mut *state {
        *profile = new_profile;
        Ok(())
    } else {
        Err("Profile can only be updated on Menu screen".to_string())
    }
}

#[tauri::command]
#[specta::specta]
/// (Screen: Menu) Start/Join a new lobby, set `join_code` to `null` to be host,
/// set it to a join code to be a client. This triggers a screen change to [AppScreen::Lobby]
async fn start_lobby(
    app: AppHandle,
    join_code: Option<String>,
    settings: GameSettings,
    state: State<'_, AppStateHandle>,
) -> Result {
    let mut state = state.write().await;
    state.start_lobby(join_code, app, settings);
    Ok(())
}

// AppState::Lobby COMMANDS

#[tauri::command]
#[specta::specta]
/// (Screen: Lobby) Get the current state of the lobby, call after receiving an update event
async fn get_lobby_state(state: State<'_, AppStateHandle>) -> Result<LobbyState> {
    let state = state.read().await;
    if let AppState::Lobby(lobby) = &*state {
        Ok(lobby.clone_state().await)
    } else {
        Err("Must be called on Lobby screen".to_string())
    }
}

#[tauri::command]
#[specta::specta]
/// (Screen: Lobby) Switch teams between seekers and hiders, returns the new [LobbyState]
async fn switch_teams(seeker: bool, state: State<'_, AppStateHandle>) -> Result<LobbyState> {
    let state = state.read().await;
    if let AppState::Lobby(lobby) = &*state {
        lobby.switch_teams(seeker).await;
        Ok(lobby.clone_state().await)
    } else {
        Err("Must be called on Lobby screen".to_string())
    }
}

#[tauri::command]
#[specta::specta]
/// (Screen: Lobby) HOST ONLY: Push new settings to everyone, does nothing on clients. Returns the
/// new lobby state
async fn host_update_settings(
    settings: GameSettings,
    state: State<'_, AppStateHandle>,
) -> Result<LobbyState> {
    let state = state.read().await;
    if let AppState::Lobby(lobby) = &*state {
        lobby.update_settings(settings).await;
        Ok(lobby.clone_state().await)
    } else {
        Err("Must be called on Lobby screen".to_string())
    }
}

#[tauri::command]
#[specta::specta]
/// (Screen: Lobby) HOST ONLY: Start the game, stops anyone else from joining and switched screen
/// to AppScreen::Game.
async fn host_start_game(state: State<'_, AppStateHandle>) -> Result {
    let state = state.read().await;
    if let AppState::Lobby(lobby) = &*state {
        lobby.start_game().await;
        Ok(())
    } else {
        Err("Must be called on Lobby screen".to_string())
    }
}

// AppScreen::Game COMMANDS

#[tauri::command]
#[specta::specta]
/// (Screen: Game) Mark this player as caught, this player will become a seeker. Returns the new game state
async fn mark_caught(state: State<'_, AppStateHandle>) -> Result<GameState> {
    let state = state.read().await;
    if let AppState::Game(game) = &*state {
        game.mark_caught().await;
        Ok(game.clone_state().await)
    } else {
        Err("Must be called on Game screen".to_string())
    }
}

#[tauri::command]
#[specta::specta]
/// (Screen: Game) Grab a powerup on the map, this should be called when the user is *in range* of
/// the powerup. Returns the new game state after rolling for the powerup
async fn grab_powerup(state: State<'_, AppStateHandle>) -> Result<GameState> {
    let state = state.read().await;
    if let AppState::Game(game) = &*state {
        game.get_powerup().await;
        Ok(game.clone_state().await)
    } else {
        Err("Must be called on Game screen".to_string())
    }
}

#[tauri::command]
#[specta::specta]
/// (Screen: Game) Use the currently held powerup in the player's held_powerup. Does nothing if the
/// player has none. Returns the updated game state
async fn use_powerup(state: State<'_, AppStateHandle>) -> Result<GameState> {
    let state = state.read().await;
    if let AppState::Game(game) = &*state {
        game.use_powerup().await;
        Ok(game.clone_state().await)
    } else {
        Err("Must be called on Game screen".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = RwLock::new(AppState::Setup);

    let builder = tauri_specta::Builder::<tauri::Wry>::new().commands(collect_commands![
        start_lobby,
        quit_game_or_lobby,
        get_current_screen,
        update_profile,
        get_lobby_state,
        host_update_settings,
        switch_teams,
        host_start_game,
        mark_caught,
        grab_powerup,
        use_powerup,
    ]);

    #[cfg(debug_assertions)]
    builder
        .export(Typescript::default(), "../frontend/src/bindings.ts")
        .expect("Failed to export typescript bindings");

    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_geolocation::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(builder.invoke_handler())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Some(profile) = PlayerProfile::load_from_store(&handle) {
                    let state_handle = handle.state::<AppStateHandle>();
                    let mut state = state_handle.write().await;
                    *state = AppState::Menu(profile);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

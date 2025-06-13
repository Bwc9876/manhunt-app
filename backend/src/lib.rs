mod game;
mod lobby;
mod location;
mod profile;
mod transport;

use std::{sync::Arc, time::Duration};

use game::{Game as BaseGame, GameSettings};
use lobby::{Lobby, StartGameInfo};
use location::TauriLocation;
use matchbox_socket::PeerId;
use profile::PlayerProfile;
use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;
use transport::MatchboxTransport;

type Game = BaseGame<PeerId, TauriLocation, MatchboxTransport>;

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
    pub fn start_game(&mut self, app: AppHandle, my_id: PeerId, start: StartGameInfo) {
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
                let app_after = app.clone();
                let lobby = Arc::new(Lobby::new(
                    server_url(),
                    &room_code,
                    app,
                    host,
                    profile.clone(),
                    settings,
                ));
                *self = AppState::Lobby(lobby.clone());
                tokio::spawn(async move {
                    let (my_id, start) = lobby.open().await;
                    let app_game = app_after.clone();
                    let state_handle = app_after.state::<AppStateHandle>();
                    let mut state = state_handle.write().await;
                    state.start_game(app_game, my_id, start);
                });
            }
            _ => {}
        }
    }
}

#[tauri::command]
async fn go_to_lobby(
    app: AppHandle,
    join_code: Option<String>,
    settings: GameSettings,
    state: State<'_, AppStateHandle>,
) -> Result<(), String> {
    let mut state = state.write().await;
    state.start_lobby(join_code, app, settings);
    Ok(())
}

#[tauri::command]
async fn host_start_game(state: State<'_, AppStateHandle>) -> Result<(), String> {
    let state = state.read().await;
    match &*state {
        AppState::Lobby(lobby) => {
            lobby.start_game().await;
            Ok(())
        }
        _ => Err("Invalid AppState".to_string()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = RwLock::new(AppState::Setup);

    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_geolocation::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            let handle = app.handle().clone();
            tokio::spawn(async move {
                if let Some(profile) = PlayerProfile::load_from_store(&handle) {
                    let state_handle = handle.state::<AppStateHandle>();
                    let mut state = state_handle.write().await;
                    *state = AppState::Menu(profile);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![go_to_lobby])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

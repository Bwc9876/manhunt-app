mod history;
mod location;
mod profiles;
mod state;

use std::collections::HashMap;

use log::LevelFilter;
use manhunt_logic::{GameSettings, GameUiState, LobbyState, PlayerProfile, UtcDT};
use manhunt_transport::room_exists;
use tauri::{AppHandle, Manager, State};
use tauri_specta::{ErrorHandlingMode, collect_commands, collect_events};
use tokio::sync::RwLock;
use uuid::Uuid;

use std::result::Result as StdResult;

use crate::{
    history::AppGameHistory,
    profiles::{read_profile_from_store, write_profile_to_store},
    state::{AppScreen, AppState, AppStateHandle, ChangeScreen, GameStateUpdate, LobbyStateUpdate},
};

type Result<T = (), E = String> = StdResult<T, E>;

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
        AppState::Game(_game, _profiles) => AppScreen::Game,
        AppState::Replay(_) => AppScreen::Replay,
    })
}

#[tauri::command]
#[specta::specta]
/// Quit a running game or leave a lobby
async fn quit_to_menu(app: AppHandle, state: State<'_, AppStateHandle>) -> Result {
    let mut state = state.write().await;
    state.quit_to_menu(app).await;
    Ok(())
}

// == AppState::Setup COMMANDS

#[tauri::command]
#[specta::specta]
/// (Screen: Setup) Complete user setup and go to the menu screen
async fn complete_setup(
    profile: PlayerProfile,
    app: AppHandle,
    state: State<'_, AppStateHandle>,
) -> Result {
    state.write().await.complete_setup(&app, profile)
}

// == AppState::Menu COMMANDS ==

#[tauri::command]
#[specta::specta]
/// (Screen: Menu) Get the user's player profile
async fn get_profile(state: State<'_, AppStateHandle>) -> Result<PlayerProfile> {
    let state = state.read().await;
    let profile = state.get_menu()?;
    Ok(profile.clone())
}

#[tauri::command]
#[specta::specta]
/// (Screen: Menu) Get a list of all previously played games, returns of list of DateTimes that represent when
/// each game started, use this as a key
fn list_game_histories(app: AppHandle) -> Result<Vec<UtcDT>> {
    AppGameHistory::ls_histories(&app)
        .map_err(|err| err.context("Failed to get game histories").to_string())
}

#[tauri::command]
#[specta::specta]
/// (Screen: Menu) Go to the game replay screen to replay the game history specified by id
async fn replay_game(id: UtcDT, app: AppHandle, state: State<'_, AppStateHandle>) -> Result {
    state.write().await.replay_game(&app, id)
}

#[tauri::command]
#[specta::specta]
/// (Screen: Menu) Check if a room code is valid to join, use this before starting a game
/// for faster error checking.
async fn check_room_code(code: &str) -> Result<bool> {
    room_exists(code).await.map_err(|err| err.to_string())
}

#[tauri::command]
#[specta::specta]
/// (Screen: Menu) Update the player's profile and persist it
async fn update_profile(
    new_profile: PlayerProfile,
    app: AppHandle,
    state: State<'_, AppStateHandle>,
) -> Result {
    write_profile_to_store(&app, new_profile.clone());
    let mut state = state.write().await;
    let profile = state.get_menu_mut()?;
    *profile = new_profile;
    Ok(())
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
    state.start_lobby(join_code, app, settings).await;
    Ok(())
}

// AppState::Lobby COMMANDS

#[tauri::command]
#[specta::specta]
/// (Screen: Lobby) Get the current state of the lobby, call after receiving an update event
async fn get_lobby_state(state: State<'_, AppStateHandle>) -> Result<LobbyState> {
    let lobby = state.read().await.get_lobby()?;
    Ok(lobby.clone_state().await)
}

#[tauri::command]
#[specta::specta]
/// (Screen: Lobby) Switch teams between seekers and hiders, returns the new [LobbyState]
async fn switch_teams(seeker: bool, state: State<'_, AppStateHandle>) -> Result {
    let lobby = state.read().await.get_lobby()?;
    lobby.switch_teams(seeker).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
/// (Screen: Lobby) HOST ONLY: Push new settings to everyone, does nothing on clients. Returns the
/// new lobby state
async fn host_update_settings(settings: GameSettings, state: State<'_, AppStateHandle>) -> Result {
    let lobby = state.read().await.get_lobby()?;
    lobby.update_settings(settings).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
/// (Screen: Lobby) HOST ONLY: Start the game, stops anyone else from joining and switched screen
/// to AppScreen::Game.
async fn host_start_game(state: State<'_, AppStateHandle>) -> Result {
    state.read().await.get_lobby()?.start_game().await;
    Ok(())
}

// AppScreen::Game COMMANDS

#[tauri::command]
#[specta::specta]
/// (Screen: Game) Get all player profiles with display names and profile pictures for this game.
/// This value will never change and is fairly expensive to clone, so please minimize calls to
/// this command.
async fn get_profiles(state: State<'_, AppStateHandle>) -> Result<HashMap<Uuid, PlayerProfile>> {
    state.read().await.get_profiles().cloned()
}

#[tauri::command]
#[specta::specta]
/// (Screen: Game) Get the current settings for this game.
async fn get_game_settings(state: State<'_, AppStateHandle>) -> Result<GameSettings> {
    Ok(state.read().await.get_game()?.clone_settings().await)
}

#[tauri::command]
#[specta::specta]
/// (Screen: Game) Get the current state of the game.
async fn get_game_state(state: State<'_, AppStateHandle>) -> Result<GameUiState> {
    Ok(state.read().await.get_game()?.get_ui_state().await)
}

#[tauri::command]
#[specta::specta]
/// (Screen: Game) Mark this player as caught, this player will become a seeker. Returns the new game state
async fn mark_caught(state: State<'_, AppStateHandle>) -> Result {
    let game = state.read().await.get_game()?;
    game.mark_caught().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
/// (Screen: Game) Grab a powerup on the map, this should be called when the user is *in range* of
/// the powerup. Returns the new game state after rolling for the powerup
async fn grab_powerup(state: State<'_, AppStateHandle>) -> Result {
    let game = state.read().await.get_game()?;
    game.get_powerup().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
/// (Screen: Game) Use the currently held powerup in the player's held_powerup. Does nothing if the
/// player has none. Returns the updated game state
async fn activate_powerup(state: State<'_, AppStateHandle>) -> Result {
    let game = state.read().await.get_game()?;
    game.use_powerup().await;
    Ok(())
}

// AppState::Replay COMMANDS

#[tauri::command]
#[specta::specta]
/// (Screen: Replay) Get the game history that's currently being replayed. Try to limit calls to
/// this
async fn get_current_replay_history(state: State<'_, AppStateHandle>) -> Result<AppGameHistory> {
    state.read().await.get_replay()
}

pub fn mk_specta() -> tauri_specta::Builder {
    tauri_specta::Builder::<tauri::Wry>::new()
        .error_handling(ErrorHandlingMode::Throw)
        .commands(collect_commands![
            start_lobby,
            get_profile,
            quit_to_menu,
            get_current_screen,
            update_profile,
            get_lobby_state,
            host_update_settings,
            switch_teams,
            host_start_game,
            mark_caught,
            grab_powerup,
            activate_powerup,
            check_room_code,
            get_profiles,
            replay_game,
            list_game_histories,
            get_current_replay_history,
            get_game_settings,
            get_game_state,
            complete_setup,
        ])
        .events(collect_events![
            ChangeScreen,
            GameStateUpdate,
            LobbyStateUpdate
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = RwLock::new(AppState::Setup);

    let builder = mk_specta();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(LevelFilter::Debug)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_geolocation::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(builder.invoke_handler())
        .manage(state)
        .setup(move |app| {
            builder.mount_events(app);

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Some(profile) = read_profile_from_store(&handle) {
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

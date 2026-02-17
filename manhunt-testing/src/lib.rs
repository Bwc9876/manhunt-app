use interprocess::local_socket::{GenericNamespaced, Name, ToNsName};
use manhunt_logic::{GameHistory, GameSettings, GameUiState, LobbyState, PowerUpType};
use serde::{Deserialize, Serialize};

pub mod prelude {
    pub use anyhow::{Context, anyhow, bail};
    pub type Result<T = (), E = anyhow::Error> = std::result::Result<T, E>;
}

pub use prelude::*;

pub fn get_socket_name(base_name: String) -> Result<Name<'static>> {
    base_name
        .to_ns_name::<GenericNamespaced>()
        .context("Failed to parse socket name")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LobbyRequest {
    SwitchTeams(bool),
    HostStartGame,
    HostUpdateSettings(GameSettings),
    Leave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameRequest {
    NextTick,
    MarkCaught,
    GetPowerup,
    UsePowerup,
    ForcePowerup(PowerUpType),
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestingRequest {
    StartLobby(GameSettings),
    JoinLobby(String),
    LobbyReq(LobbyRequest),
    GameReq(GameRequest),
}

impl From<LobbyRequest> for TestingRequest {
    fn from(val: LobbyRequest) -> Self {
        TestingRequest::LobbyReq(val)
    }
}

impl From<GameRequest> for TestingRequest {
    fn from(val: GameRequest) -> Self {
        TestingRequest::GameReq(val)
    }
}

impl TryInto<LobbyRequest> for TestingRequest {
    type Error = TestingResponse;

    fn try_into(self) -> Result<LobbyRequest, Self::Error> {
        if let Self::LobbyReq(lr) = self {
            Ok(lr)
        } else {
            Err(TestingResponse::WrongScreen)
        }
    }
}

impl TryInto<GameRequest> for TestingRequest {
    type Error = TestingResponse;

    fn try_into(self) -> Result<GameRequest, Self::Error> {
        if let Self::GameReq(gr) = self {
            Ok(gr)
        } else {
            Err(TestingResponse::WrongScreen)
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScreenUpdate {
    PreConnect,
    Lobby,
    Game,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestingResponse {
    Complete,
    ScreenChanged(ScreenUpdate),
    LobbyStateUpdate(LobbyState),
    GameStateUpdate(GameUiState),
    GameOver(GameHistory),
    WrongScreen,
    Error(String),
}

impl From<GameHistory> for TestingResponse {
    fn from(val: GameHistory) -> Self {
        TestingResponse::GameOver(val)
    }
}

impl From<anyhow::Error> for TestingResponse {
    fn from(value: anyhow::Error) -> Self {
        TestingResponse::Error(value.to_string())
    }
}

impl From<ScreenUpdate> for TestingResponse {
    fn from(val: ScreenUpdate) -> Self {
        TestingResponse::ScreenChanged(val)
    }
}

impl From<LobbyState> for TestingResponse {
    fn from(val: LobbyState) -> Self {
        TestingResponse::LobbyStateUpdate(val)
    }
}

impl From<GameUiState> for TestingResponse {
    fn from(val: GameUiState) -> Self {
        TestingResponse::GameStateUpdate(val)
    }
}

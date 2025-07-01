#![allow(clippy::result_large_err)]

use manhunt_logic::{
    Game as BaseGame, GameSettings, Lobby as BaseLobby, Location, LocationService, PlayerProfile,
    StartGameInfo, StateUpdateSender,
};
use manhunt_test_shared::*;
use manhunt_transport::{MatchboxTransport, request_room_code};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, mpsc},
};

struct DummyLocationService;

impl LocationService for DummyLocationService {
    fn get_loc(&self) -> Option<manhunt_logic::Location> {
        Some(Location {
            lat: 0.0,
            long: 0.0,
            heading: None,
        })
    }
}

struct UpdateSender(mpsc::Sender<()>);

impl StateUpdateSender for UpdateSender {
    fn send_update(&self) {
        let tx = self.0.clone();
        tokio::spawn(async move {
            tx.send(()).await.expect("Failed to send");
        });
    }
}

type Game = BaseGame<DummyLocationService, MatchboxTransport, UpdateSender>;
type Lobby = BaseLobby<MatchboxTransport, UpdateSender>;

#[derive(Default)]
enum DaemonScreen {
    #[default]
    PreConnect,
    Lobby(Arc<Lobby>),
    Game(Arc<Game>),
}

impl DaemonScreen {
    pub fn as_update(&self) -> ScreenUpdate {
        match self {
            Self::PreConnect => ScreenUpdate::PreConnect,
            Self::Game(_) => ScreenUpdate::Game,
            Self::Lobby(_) => ScreenUpdate::Lobby,
        }
    }
}

type StateHandle = Arc<Mutex<DaemonState>>;

struct DaemonState {
    screen: DaemonScreen,
    profile: PlayerProfile,
    responses: mpsc::Sender<TestingResponse>,
    updates: (mpsc::Sender<()>, Mutex<mpsc::Receiver<()>>),
}

impl DaemonState {
    pub fn new(name: impl Into<String>, responses: mpsc::Sender<TestingResponse>) -> Self {
        tokio::time::pause();
        let screen = DaemonScreen::default();
        let (tx, rx) = mpsc::channel(2);
        Self {
            screen,
            responses,
            profile: PlayerProfile {
                display_name: name.into(),
                pfp_base64: None,
            },
            updates: (tx, Mutex::new(rx)),
        }
    }

    async fn change_screen(&mut self, new_screen: DaemonScreen) {
        let update = new_screen.as_update();
        self.screen = new_screen;
        self.push_resp(update).await;
    }

    async fn lobby_loop(&self, handle: StateHandle) {
        if let DaemonScreen::Lobby(lobby) = &self.screen {
            let lobby = lobby.clone();
            tokio::spawn(async move {
                let res = lobby.main_loop().await;
                let handle2 = handle.clone();
                let mut state = handle.lock().await;
                match res {
                    Ok(Some(start)) => {
                        state.start_game(handle2, start).await;
                    }
                    Ok(None) => {
                        state.change_screen(DaemonScreen::PreConnect).await;
                    }
                    Err(why) => {
                        state.push_resp(why).await;
                        state.change_screen(DaemonScreen::PreConnect).await;
                    }
                }
            });
        }
    }

    async fn game_loop(&self, handle: StateHandle) {
        if let DaemonScreen::Game(game) = &self.screen {
            let game = game.clone();
            tokio::spawn(async move {
                let res = game.main_loop().await;
                let mut state = handle.lock().await;
                match res {
                    Ok(Some(history)) => {
                        state.push_resp(history).await;
                    }
                    Ok(None) => {}
                    Err(why) => {
                        state.push_resp(why).await;
                    }
                }
                state.change_screen(DaemonScreen::PreConnect).await;
            });
        }
    }

    async fn push_resp(&self, resp: impl Into<TestingResponse>) {
        self.responses
            .send(resp.into())
            .await
            .expect("Failed to push response");
    }

    fn sender(&self) -> UpdateSender {
        UpdateSender(self.updates.0.clone())
    }

    const INTERVAL: Duration = Duration::from_secs(1);

    async fn start_game(&mut self, handle: StateHandle, start: StartGameInfo) {
        if let DaemonScreen::Lobby(lobby) = &self.screen {
            let transport = lobby.clone_transport();
            let updates = self.sender();
            let location = DummyLocationService;

            let game = Game::new(Self::INTERVAL, start, transport, location, updates);

            self.change_screen(DaemonScreen::Game(Arc::new(game))).await;
            self.game_loop(handle).await;
        }
    }

    pub async fn create_lobby(&mut self, handle: StateHandle, settings: GameSettings) -> Result {
        let sender = self.sender();

        let code = request_room_code()
            .await
            .context("Failed to get room code")?;

        let lobby = Lobby::new(&code, true, self.profile.clone(), settings, sender)
            .await
            .context("Failed to start lobby")?;

        self.change_screen(DaemonScreen::Lobby(lobby)).await;
        self.lobby_loop(handle).await;

        Ok(())
    }

    pub async fn join_lobby(&mut self, handle: StateHandle, code: &str) -> Result {
        let sender = self.sender();
        // TODO: Lobby should not require this on join, use an [Option]?
        let settings = GameSettings::default();

        let lobby = Lobby::new(code, false, self.profile.clone(), settings, sender)
            .await
            .context("Failed to join lobby")?;

        self.change_screen(DaemonScreen::Lobby(lobby)).await;
        self.lobby_loop(handle).await;

        Ok(())
    }

    fn assert_screen(&self, expected: ScreenUpdate) -> Result<(), TestingResponse> {
        if self.screen.as_update() == expected {
            Ok(())
        } else {
            Err(TestingResponse::WrongScreen)
        }
    }

    async fn process_lobby_req(&mut self, req: LobbyRequest) {
        if let DaemonScreen::Lobby(lobby) = &self.screen {
            let lobby = lobby.clone();
            match req {
                LobbyRequest::SwitchTeams(seeker) => lobby.switch_teams(seeker).await,
                LobbyRequest::HostStartGame => lobby.start_game().await,
                LobbyRequest::HostUpdateSettings(game_settings) => {
                    lobby.update_settings(game_settings).await
                }
                LobbyRequest::Leave => lobby.quit_lobby().await,
            }
        }
    }

    async fn process_game_req(&mut self, req: GameRequest) {
        if let DaemonScreen::Game(game) = &self.screen {
            let game = game.clone();
            match req {
                GameRequest::NextTick => tokio::time::sleep(Self::INTERVAL).await,
                GameRequest::MarkCaught => game.mark_caught().await,
                GameRequest::GetPowerup => game.get_powerup().await,
                GameRequest::UsePowerup => game.use_powerup().await,
                GameRequest::ForcePowerup(power_up_type) => {
                    let mut state = game.lock_state().await;
                    state.force_set_powerup(power_up_type);
                }
                GameRequest::Quit => game.quit_game().await,
            }
        }
    }

    pub async fn process_req(
        &mut self,
        handle: StateHandle,
        req: TestingRequest,
    ) -> Result<(), TestingResponse> {
        match req {
            TestingRequest::StartLobby(game_settings) => {
                self.assert_screen(ScreenUpdate::PreConnect)?;
                self.create_lobby(handle, game_settings).await?;
            }
            TestingRequest::JoinLobby(code) => {
                self.assert_screen(ScreenUpdate::PreConnect)?;
                self.join_lobby(handle, &code).await?;
            }
            TestingRequest::LobbyReq(lobby_request) => {
                self.assert_screen(ScreenUpdate::Lobby)?;
                self.process_lobby_req(lobby_request).await;
            }
            TestingRequest::GameReq(game_request) => {
                self.assert_screen(ScreenUpdate::Game)?;
                self.process_game_req(game_request).await;
            }
        }
        Ok(())
    }
}

use interprocess::local_socket::{ListenerOptions, tokio::prelude::*};

const CLI_MSG: &str = "Usage: manhunt-test-daemon SOCKET_NAME PLAYER_NAME";

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result {
    let args = std::env::args().collect::<Vec<_>>();
    let raw_socket_name = args.get(1).cloned().expect(CLI_MSG);
    let player_name = args.get(2).cloned().expect(CLI_MSG);
    let socket_name = get_socket_name(raw_socket_name)?;
    let opts = ListenerOptions::new().name(socket_name);
    let listener = opts.create_tokio().context("Failed to bind to socket")?;
    let (resp_tx, mut resp_rx) = mpsc::channel::<TestingResponse>(40);

    let handle = Arc::new(Mutex::new(DaemonState::new(player_name, resp_tx)));

    eprintln!("Testing Daemon Ready");

    'server: loop {
        let res = tokio::select! {
            res = listener.accept() => {
                res
            },
            Ok(_) = tokio::signal::ctrl_c() => {
                break 'server;
            }
        };

        match res {
            Ok(stream) => {
                let mut recv = BufReader::new(&stream);
                let mut send = &stream;

                let mut buffer = String::with_capacity(256);

                loop {
                    tokio::select! {
                        Ok(_) = tokio::signal::ctrl_c() => {
                            break 'server;
                        }
                        res = recv.read_line(&mut buffer) => {
                            match res {
                                Ok(0) => {
                                    break;
                                }
                                Ok(_amnt) => {
                                    let req = serde_json::from_str(&buffer).expect("Failed to parse");
                                    buffer.clear();
                                    let handle2 = handle.clone();
                                    let mut state = handle.lock().await;
                                    if let Err(resp) = state.process_req(handle2, req).await {
                                        let encoded = serde_json::to_vec(&resp).expect("Failed to encode");
                                        send.write_all(&encoded).await.expect("Failed to send");
                                    }
                                }
                                Err(why) => {
                                    eprintln!("Read Error: {why:?}");
                                }
                            }
                        }
                        Some(resp) = resp_rx.recv() => {
                            let encoded = serde_json::to_vec(&resp).expect("Failed to encode");
                            send.write_all(&encoded).await.expect("Failed to send");
                        }
                    }
                }
            }
            Err(why) => eprintln!("Error from connection: {why:?}"),
        }
    }

    Ok(())
}

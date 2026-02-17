use clap::{Parser, Subcommand, ValueEnum};
use interprocess::local_socket::{tokio::Stream, traits::tokio::Stream as _};
use manhunt_logic::PowerUpType;
use manhunt_test_shared::{get_socket_name, prelude::*};

#[derive(Parser)]
struct Cli {
    /// Path to the UNIX domain socket the test daemon is listening on
    socket: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Role {
    Seeker,
    Hider,
}

#[derive(Subcommand)]
enum LobbyCommand {
    /// Switch teams between seekers and hiders
    SwitchTeams {
        /// The role you want to become
        #[arg(value_enum)]
        role: Role,
    },
    /// (Host) Sync game settings to players
    SyncSettings,
    /// (Host) Start the game for everyone
    StartGame,
    /// Quit to the main menu
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum PowerUpTypeValue {
    PingSeeker,
    PingAllSeekers,
    ForcePingOther,
}

impl From<PowerUpTypeValue> for PowerUpType {
    fn from(value: PowerUpTypeValue) -> Self {
        match value {
            PowerUpTypeValue::PingSeeker => PowerUpType::PingSeeker,
            PowerUpTypeValue::PingAllSeekers => PowerUpType::PingAllSeekers,
            PowerUpTypeValue::ForcePingOther => PowerUpType::ForcePingOther,
        }
    }
}

#[derive(Subcommand)]
enum GameCommand {
    /// Mark the local player as caught for everyone
    MarkCaught,
    /// Get a currently available powerup
    GetPowerup,
    /// Use the held powerup of the local player
    UsePowerup,
    /// Force set the held powerup to the given type
    ForcePowerup {
        #[arg(value_enum)]
        ptype: PowerUpTypeValue,
    },
    /// Quit the game
    Quit,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a lobby
    Create,
    /// Join a lobby
    Join {
        /// The join code for the lobby
        join_code: String,
    },
    /// Execute a command in an active lobby
    #[command(subcommand)]
    Lobby(LobbyCommand),
    /// Execute a command in an active game
    #[command(subcommand)]
    Game(GameCommand),
}

#[tokio::main]
async fn main() -> Result {
    let cli = Cli::parse();

    let socket_name = get_socket_name(cli.socket.clone()).context("Failed to get socket name")?;

    let _stream = Stream::connect(socket_name)
        .await
        .context("Failed to connect to socket")?;

    Ok(())
}

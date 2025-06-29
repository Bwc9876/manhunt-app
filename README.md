# Man Hunt

[![built with garnix](https://img.shields.io/endpoint.svg?url=https%3A%2F%2Fgarnix.io%2Fapi%2Fbadges%2FBwc9876%2Fmanhunt-app%3Fbranch%3Dmain)](https://garnix.io/repo/Bwc9876/manhunt-app)

An iOS and Android app that lets you play [man hunt](<https://en.wikipedia.org/wiki/Manhunt_(urban_game)>) with friends.

The game is played over WebRTC Data Channels and is entirely P2P (except for the
signaling process needed for WebRTC.)

## Features

- Play games with friends by starting a lobby, your friends can join with the
  room code on the lobby screen.
- Pings can be configured to happen at certain intervals. They reveal hider
  locations so hiders have to move.
- Powerups are random items you can grab in certain locations that can deceive
  seekers or mess up other hiders.
- Watch a replay of your game after it ends, it shows everyone's location
  throughout the game.
- Your location data is safe, it's only ever sent to the other players in the game.

<!-- TODO: Download & Install instructions for when we get to publishing -->

## Development

### Pre-requisites

If you have [nix](https://nixos.org) installed, all of these are handled for you.

- [Rust](https://rustup.rs)
- [Just](https://just.systems) (`cargo install just`)
    - **On Windows**: Some implementation of `sh` (Git for Windows works well)
- Cargo Nextest (only needed for running `just check-rust`, `cargo install cargo-nextest`)
- [Tauri's Pre-reqs](https://tauri.app/start/prerequisites/)
    - [(Also pre-reqs for mobile dev if you are working on the app part)](https://tauri.app/start/prerequisites/#configure-for-mobile-targets)
- Tauri's CLI (`cargo install tauri-cli`)
- [NodeJS](https://nodejs.org)
- [Prettier](https://prettier.io/) (`npm add -g prettier`)

#### With Nix

Run `nix develop` to get a development shell with all needed dependencies set up.
You can then call the `just` recipes mentioned below within the shell.

### Setup

- Rust is ready to go
- For the frontend, run `just setup-frontend` to install the dependencies via `npm`

### Run App

- `just dev`: Run the app locally on your computer, this will open a
  WebView with the frontend for testing.
    - Note: all geolocation data returned from `tauri-plugin-geolocation` will be hard
      coded to `(0.0, 0.0)` in this mode.
- `just dev-android`: Run the app on an Android device or VM via ADB
- `just signaling`: Will run the signaling server on port `3536`
  (this is needed for clients to connect).
  If you need a different port run `cargo run --bin manhunt-signaling 0.0.0.0:PORT`.

### Project Layout

- [manhunt-logic/](https://github.com/Bwc9876/manhunt-app/tree/main/manhunt-logic):
  Game and lobby logic for the app
- [manhunt-transport/](https://github.com/Bwc9876/manhunt-app/tree/main/manhunt-transport):
  Transport (networking) implementation for communication between apps
- [manhunt-app/](https://github.com/Bwc9876/manhunt-app/tree/main/manhunt-app): App
  backend, Rust side of the Tauri application
- [frontend/](https://github.com/Bwc9876/manhunt-app/tree/main/frontend): App
  frontend, Web side of the Tauri application
- [nix/](https://github.com/Bwc9876/manhunt-app/tree/main/nix): Nix files for
  the flake
- [manhunt-signaling/](https://github.com/Bwc9876/manhunt-app/tree/main/manhunt-signaling):
  Matchbox signaling server implementation in Rust

### Housekeeping

As you go, please run these `just` commands every-so-often and before you commit:

- `just fmt`: Formats all files in the repo
- `just check-rust`: Check (and fix) potential issues with Rust code
  (only need to run if you edited rust code)
- `just check-frontend`: Check for potential issues on the frontend
  (only need to run if you edited the frontend)

**Important**: When changing any type in a rust file that derives `specta::Type`,
you need to run `just export-types` to sync these type bindings to the frontend.
Otherwise the TypeScript definitions will not match the ones that the backend expects.

All changes will be put through CI to check that all of these commands have
been done.

### Other Just Recipes

Run `just` without any args to get a list.

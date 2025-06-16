# Man Hunt

[![built with garnix](https://img.shields.io/endpoint.svg?url=https%3A%2F%2Fgarnix.io%2Fapi%2Fbadges%2FBwc9876%2Fmanhunt-app%3Fbranch%3Dmain)](https://garnix.io/repo/Bwc9876/manhunt-app)

An iOS and Android app that lets you play man hunt with friends.

The game is played over WebRTC Data Channels and is entirely P2P (except for the
signaling process needed for WebRTC.)

<!-- TODO: Download & Install instructions for when we get to publishing -->

## Development

### Pre-requisites

If you have [nix](https://nixos.org) installed, all of these are handled for you.

- [Rust](https://rustup.rs)
- [Just](https://just.systems) (`cargo install just`)
    - **On Windows**: Some implementation of `sh` (Git for Windows works well)
- [Tauri's Pre-reqs](https://tauri.app/start/prerequisites/)
    - [(Also pre-reqs for mobile dev if you are working on the app part)](https://tauri.app/start/prerequisites/#configure-for-mobile-targets)
- Tauri's CLI (`cargo install tauri-cli`)
- [NodeJS](https://nodejs.org)
- [Prettier](https://prettier.io/) (`npm add -g prettier`)

#### With Nix

Run `nix develop` to get a development shell with all needed dependencies set up.
You can then call the `just` recipes mentioned below.

### Setup

- Rust is ready to go
- For the frontend, run `just setup-frontend` to install the dependencies via `npm`

### Run App

- `just dev`: Will run the app locally on your computer, this will open a
  WebView with the frontend
    - Note: all geolocation returned from tauri-plugin-geolocation will be hard
      coded to `(0.0, 0.0)` in this mode.
- `just dev-android`: Will run the app on a connect Android device or VM via ADB
- `just signaling`: Will run the signaling server on port `3536`
  (this is needed for clients to connect)

### Project Layout

- [backend/](https://github.com/Bwc9876/manhunt-app/tree/main/backend): App
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
- `just check-backend`: Check (and fix) potential issues on the backend
  (only need to run if you edited the backend)
- `just check-frontend`: Check for potential issues on the frontend
  (only need to run if you edited the frontend)
- `just check-signaling`: Same thing as backend but for the singaling server

**Important**: When changing any type in `backend` that derives `specta::Type`,
you need to run `just export-types` to sync these type bindings to the frontend,
otherwise the TypeScript definitions will not match that ones the backend expects.

All changes made will be put through CI to check that all of these commands have
been done.

### Other Just Recipes

Run `just` without any args to get a list.

# TODO

## Ben

- [x] Transport : Packet splitting
- [x] Transport : Handle Errors
- [x] Transport : Mark game started on client
- [x] API : Command to check if a game exists and is open for fast error checking
- [x] Transport : Switch to burst message processing for less time in the
      critical path
- [x] State : Event history tracking
- [x] State : Post game sync
- [x] API : Handling Profile Syncing
- [x] API : State Update Events
- [x] API : Game Replay Screen
- [x] Frontend : Scaffolding
- [x] Meta : CI Setup
- [x] Meta : README Instructions
- [x] Meta : Recipes for type binding generation
- [x] Signaling: All of it
- [x] Backend : Better transport error handling
- [x] Backend : Abstract lobby? Separate crate?
- [x] Transport : Handle transport cancellation better
- [x] Backend : Add checks for when the `powerup_locations` field is an empty array in settings
- [ ] Backend : More tests
    - [ ] Lobby tests
    - [ ] Game end test for actual return from loop
    - [ ] Testing crate for integration testing from a DSL
    - [ ] NixOS VM tests wrapping the testing crate
- [ ] Nix : Cheat the dependency nightmare and use crane
- [x] Nix : Fix manhunt.nix to actually build
- [ ] Frontend : Rework state management, better hooks

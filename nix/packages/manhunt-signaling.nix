{
  lib,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  pname = "manhunt-signaling";
  version = "0.1.0";
  src = with lib.fileset;
    toSource {
      root = ../../.;
      fileset = unions [
        ../../backend
        ../../manhunt-logic
        ../../manhunt-transport
        ../../manhunt-signaling
        ../../Cargo.toml
        ../../Cargo.lock
      ];
    };
  cargoLock.lockFile = ../../Cargo.lock;
  buildAndTestSubdir = "manhunt-signaling";

  postPatch = ''
    cp ${../../Cargo.lock} Cargo.lock
    chmod +w Cargo.lock
  '';

  meta = with lib; {
    description = "Signaling server for Manhunt app";
    mainProgram = "manhunt-signaling";
    platforms = platforms.linux;
    license = licenses.gpl3;
    maintainers = with maintainers; [bwc9876];
  };
}

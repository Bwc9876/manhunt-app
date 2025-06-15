{
  lib,
  clippy,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  pname = "manhunt-signaling";
  version = "0.1.0";
  src = with lib.fileset;
    toSource {
      root = ../../.;
      fileset = unions [
        ../../Cargo.toml
        ../../Cargo.lock
        ../../backend
        ../../manhunt-signaling
      ];
    };
  cargoLock.lockFile = ../../Cargo.lock;
  buildAndTestSubdir = "manhunt-signaling";

  postCheck = ''
    cargo clippy -p manhunt-signaling --no-deps -- -D warnings
  '';

  nativeBuildInputs = [clippy];

  meta = with lib; {
    description = "Signaling server for Manhunt app";
    mainProgram = "manhunt-signaling";
    platforms = platforms.linux;
    license = licenses.gpl3;
    maintainers = with maintainers; [bwc9876];
  };
}

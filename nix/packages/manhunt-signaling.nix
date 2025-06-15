{
  lib,
  clippy,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  pname = "manhunt-signaling";
  version = "0.1.0";
  src = ../../manhunt-signaling;
  cargoLock.lockFile = ../../Cargo.lock;

  postPatch = ''
    cp ${../../Cargo.lock} Cargo.lock
    chmod +w Cargo.lock
  '';

  postCheck = ''
    cargo clippy --no-deps -- -D warnings
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

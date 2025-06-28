{
  lib,
  libsoup_3,
  dbus,
  glib,
  glib-networking,
  librsvg,
  webkitgtk_4_1,
  pkg-config,
  wrapGAppsHook,
  copyDesktopItems,
  rustPlatform,
  manhunt-frontend,
  cargo-nextest,
}:
rustPlatform.buildRustPackage {
  pname = "manhunt";
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
  buildAndTestSubdir = "backend";
  buildFeatures = [
    "tauri/custom-protocol"
  ];

  nativeBuildInputs = [
    pkg-config
    copyDesktopItems
    wrapGAppsHook
  ];

  buildInputs = [
    dbus
    libsoup_3
    glib
    librsvg
    glib-networking
    webkitgtk_4_1
  ];

  postPatch = ''
    substituteInPlace backend/tauri.conf.json \
    --replace-fail '"frontendDist": "../frontend/dist"' '"frontendDist": "${manhunt-frontend}"'
  '';

  useNextest = true;

  cargoTestFlags = "-p manhunt-logic -p manhunt-transport -p manhunt-app";

  postInstall = ''
    install -DT backend/icons/128x128@2x.png $out/share/icons/hicolor/256x256@2/apps/manhunt.png
    install -DT backend/icons/128x128.png $out/share/icons/hicolor/128x128/apps/manhunt.png
    install -DT backend/icons/32x32.png $out/share/icons/hicolor/32x32/apps/manhunt.png
  '';

  meta = with lib; {
    description = "Manhunt app";
    mainProgram = "manhunt-app";
    platforms = platforms.linux;
    license = licenses.gpl3;
    maintainers = with maintainers; [bwc9876];
  };
}

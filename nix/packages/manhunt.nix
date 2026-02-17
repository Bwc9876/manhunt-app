{
  lib,
  libsoup_3,
  dbus,
  glib,
  glib-networking,
  librsvg,
  webkitgtk_4_1,
  pkg-config,
  wrapGAppsHook3,
  copyDesktopItems,
  rustPlatform,
  manhunt-frontend,
}:
rustPlatform.buildRustPackage {
  pname = "manhunt";
  version = "0.1.0";
  src = with lib.fileset;
    toSource {
      root = ../../.;
      fileset = unions [
        ../../manhunt-app
        ../../manhunt-logic
        ../../manhunt-transport
        ../../manhunt-signaling
        ../../manhunt-testing
        ../../Cargo.toml
        ../../Cargo.lock
      ];
    };
  cargoLock.lockFile = ../../Cargo.lock;
  buildAndTestSubdir = "manhunt-app";
  buildFeatures = [
    "tauri/custom-protocol"
  ];

  nativeBuildInputs = [
    pkg-config
    copyDesktopItems
    wrapGAppsHook3
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
    substituteInPlace manhunt-app/tauri.conf.json \
    --replace-fail '"frontendDist": "../frontend/dist"' '"frontendDist": "${manhunt-frontend}"'
  '';

  useNextest = true;

  cargoTestFlags = "-p manhunt-logic -p manhunt-transport -p manhunt-app";

  postInstall = ''
    install -DT manhunt-app/icons/128x128@2x.png $out/share/icons/hicolor/256x256@2/apps/manhunt.png
    install -DT manhunt-app/icons/128x128.png $out/share/icons/hicolor/128x128/apps/manhunt.png
    install -DT manhunt-app/icons/32x32.png $out/share/icons/hicolor/32x32/apps/manhunt.png
  '';

  meta = with lib; {
    description = "Manhunt app";
    mainProgram = "manhunt-app";
    platforms = platforms.linux;
    license = licenses.gpl3;
    maintainers = with maintainers; [bwc9876];
  };
}

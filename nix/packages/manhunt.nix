{
  lib,
  libsoup_3,
  dbus,
  glib,
  clippy,
  glib-networking,
  librsvg,
  webkitgtk_4_1,
  pkg-config,
  wrapGAppsHook,
  copyDesktopItems,
  rustPlatform,
  manhunt-frontend,
}:
rustPlatform.buildRustPackage {
  pname = "manhunt";
  version = "0.1.0";
  src = ../../backend;
  cargoLock.lockFile = ../../Cargo.lock;
  buildFeatures = [
    "tauri/custom-protocol"
  ];

  postCheck = ''
    cargo clippy --no-deps -- -D warnings
  '';

  nativeBuildInputs = [
    pkg-config
    copyDesktopItems
    wrapGAppsHook
    clippy
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
    cp ${../../Cargo.lock} Cargo.lock
    chmod +w Cargo.lock

    substituteInPlace tauri.conf.json \
    --replace '"frontendDist": "../frontend/dist"' '"frontendDist": "${manhunt-frontend}"'
  '';

  postInstall = ''
    install -DT icons/128x128@2x.png $out/share/icons/hicolor/256x256@2/apps/manhunt.png
    install -DT icons/128x128.png $out/share/icons/hicolor/128x128/apps/manhunt.png
    install -DT icons/32x32.png $out/share/icons/hicolor/32x32/apps/manhunt.png
  '';

  meta = with lib; {
    description = "Manhunt app";
    mainProgram = "manhunt-app";
    platforms = platforms.linux;
    license = licenses.gpl3;
    maintainers = with maintainers; [bwc9876];
  };
}

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
}:
rustPlatform.buildRustPackage {
  pname = "manhunt";
  version = "0.1.0";
  # TODO: fileset
  src = ../../backend;
  cargoLock.lockFile = ../../backend/Cargo.lock;
  buildFeatures = [
    "tauri/custom-protocol"
  ];
  doCheck = false;

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

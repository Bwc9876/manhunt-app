{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flakelight.url = "github:nix-community/flakelight";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs = {flakelight, ...} @ inputs:
    flakelight ./. {
      inherit inputs;
      withOverlays = [inputs.rust-overlay.overlays.default];
      nixpkgs.config = {
        allowUnfree = true;
        android_sdk.accept_license = true;
      };

      flakelight.builtinFormatters = false;
      formatters = {
        "*.nix" = "alejandra .";
        "*.{js,ts,jsx,tsx,md,json}" = "prettier --write . --config frontend/.prettierrc.yaml";
        "*.rs" = "cd backend; cargo fmt";
      };

      devShell = pkgs: let
        buildToolsVersion = "34.0.0";
        androidComposition = pkgs.androidenv.composeAndroidPackages {
          platformVersions = [
            "34"
            "latest"
          ];
          systemImageTypes = ["google_apis_playstore"];
          buildToolsVersions = [buildToolsVersion];
          abiVersions = [
            "armeabi-v7a"
            "arm64-v8a"
            "x86_64"
          ];
          includeNDK = true;
          includeExtras = [
            "extras;google;auto"
          ];
        };
      in {
        shellHook = let
          ANDROID_HOME = "${androidComposition.androidsdk}/libexec/android-sdk";
        in ''
          export XDG_DATA_DIRS="$GSETTINGS_SCHEMAS_PATH"
          export GIO_EXTRA_MODULES="${pkgs.dconf.lib}/lib/gio/modules:${pkgs.glib-networking}/lib/gio/modules"
          export ANDROID_HOME=${ANDROID_HOME}
          export NDK_HOME="${androidComposition.androidsdk}/libexec/android-sdk/ndk/${builtins.head (pkgs.lib.lists.reverseList (builtins.split "-" "${androidComposition.ndk-bundle}"))}"
          export GRADLE_OPTS="-Dorg.gradle.project.android.aapt2FromMavenOverride=${ANDROID_HOME}/build-tools/${buildToolsVersion}/aapt2"
        '';
        packages = with pkgs; [
          at-spi2-atk
          atkmm
          cairo
          gdk-pixbuf
          glib
          gtk3
          alejandra
          harfbuzz
          librsvg
          libsoup_3
          pango
          webkitgtk_4_1
          openssl
          pkg-config
          gobject-introspection
          nodePackages.prettier
          (rust-bin.stable.latest.default.override {targets = ["aarch64-linux-android" "armv7-linux-androideabi" "i686-linux-android" "x86_64-linux-android"];})
          cargo-tauri
          nodejs
          (android-studio.withSdk androidComposition.androidsdk)
        ];
      };
    };
}

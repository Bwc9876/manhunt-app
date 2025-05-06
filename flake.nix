{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flakelight.url = "github:nix-community/flakelight";
  };
  outputs = { flakelight, ... } @ inputs:
    flakelight ./. {
      inherit inputs;
      formatters =
        let
          forAllTypes = cmd: types:
            builtins.listToAttrs (builtins.map
              (t: {
                name = "*.${t}";
                value = cmd;
              })
              types);
        in
        {
          "*.rs" = "cd backend; cargo fmt";
        }
        // (forAllTypes "prettier --write ." [ "ts" "tsx" "md" "json" ]);
      devShell = {
        shellHook = pkgs: ''
          export XDG_DATA_DIRS="$GSETTINGS_SCHEMAS_PATH"
          export GIO_EXTRA_MODULES="${pkgs.dconf.lib}/lib/gio/modules:${pkgs.glib-networking}/lib/gio/modules"
        '';
        packages = pkgs:
          with pkgs; [
            at-spi2-atk
            atkmm
            cairo
            gdk-pixbuf
            glib
            gtk3
            harfbuzz
            librsvg
            libsoup_3
            pango
            webkitgtk_4_1
            openssl
            pkg-config
            gobject-introspection
            nodePackages.prettier
            cargo
            cargo-tauri
            nodejs
          ];
      };
    };
}

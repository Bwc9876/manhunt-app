{
  buildNpmPackage,
  importNpmLock,
}: let
  src = ../../frontend;
in
  buildNpmPackage {
    inherit src;
    pname = "manhunt-frontend";
    version = "0.0.1";
    packageJSON = ../../frontend/package.json;
    npmDeps = importNpmLock {
      npmRoot = src;
    };
    npmConfigHook = importNpmLock.npmConfigHook;

    installPhase = ''
      cp -r dist/ $out
    '';
    distPhase = "true";
    distDir = "dist";
  }

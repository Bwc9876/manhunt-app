{pkgs, ...}:
pkgs.manhunt-frontend.overrideAttrs (old: {
  name = "manhunt-frontend-lint";
  installPhase = "mkdir $out";
  npmBuildScript = "lint";
  distDir = null;
})

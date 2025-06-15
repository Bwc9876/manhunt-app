{pkgs, ...}:
pkgs.runCommand "check-types-synced" {} ''
  ${pkgs.manhunt}/bin/export-types ./bindings.ts
  ${pkgs.prettier}/bin/prettier --write ./bindings.ts --config ${../../frontend/.prettierrc.yaml}
  diff bindings.ts ${../../frontend/src/bindings.ts}
  touch $out
''

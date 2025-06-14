_default:
    @just --list --unsorted --justfile {{justfile()}}

# Run the development server in mprocs
dev:
    nix develop --command cargo tauri dev

dev-android:
    nix develop -c cargo tauri android dev

# Run a development shell
shell:
    nix develop

# Execute a single command within the shell
run *CMD:
    nix develop --command {{CMD}}

# Run an npm command within frontend
[working-directory: 'frontend']
npm *CMD:
    nix develop --command npm {{CMD}}

# Run a cargo command within backend
[working-directory: 'backend']
cargo *CMD:
    nix develop --command cargo {{CMD}}

# Export types from the backend to TypeScript bindings
[working-directory: 'backend']
export-types:
    nix develop --command cargo run --bin export-types ../frontend/src/bindings.ts



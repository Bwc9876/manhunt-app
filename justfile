_default:
    @just --list --unsorted --justfile {{justfile()}}

[working-directory: 'frontend']
# Perform setup for the frontend using `npm`
setup-frontend:
    npm install --no-fund --no-audit

# Run locally
dev:
    cargo tauri dev

# Connect and run on an Android VM/Physical device
dev-android:
    cargo tauri android dev

[working-directory: 'backend']
# Run a check on the backend
check-backend:
    cargo check
    cargo clippy --fix --allow-dirty --allow-staged -- -D warnings

[working-directory: 'frontend']
# Run lint on the frontend
check-frontend:
    npm run lint

# Export types from the backend to TypeScript bindings
[working-directory: 'backend']
export-types:
    cargo run --bin export-types ../frontend/src/bindings.ts 
    prettier --write ../frontend/src/bindings.ts


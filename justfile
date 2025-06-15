_default:
    @just --list --unsorted --justfile {{justfile()}}


# Perform setup for the frontend using `npm`
[working-directory: 'frontend']
setup-frontend:
    npm install --no-fund --no-audit

# Run locally
dev:
    cargo tauri dev

# Format everything
fmt:
    cd backend && cargo fmt
    cd frontend && npm run format

# Connect and run on an Android VM/Physical device
dev-android:
    cargo tauri android dev

# Run a check on the backend
[working-directory: 'backend']
check-backend:
    cargo check
    cargo clippy --fix --allow-dirty --allow-staged -- -D warnings


# Run lint on the frontend
[working-directory: 'frontend']
check-frontend:
    npm run lint

# Export types from the backend to TypeScript bindings
[working-directory: 'backend']
export-types:
    cargo run --bin export-types ../frontend/src/bindings.ts 
    prettier --write ../frontend/src/bindings.ts


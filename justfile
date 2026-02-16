default:
    @just --list

# Run backend with cargo watch, serving pre-built frontend from frontend-dist/
dev:
    DOCKRU_STACKS_DIR=./stacks DOCKRU_ENABLE_CONSOLE=true cargo watch -x run

# Run both backend and frontend in development mode
dev-all:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Starting development servers..."
    echo "Backend will run on http://localhost:5001"
    echo "Frontend dev server will run on http://localhost:5173"
    cargo build && cd frontend && npm install && cd .. && npx concurrently -k -r "DOCKRU_STACKS_DIR=./stacks cargo watch -x run" "cd frontend && npm run dev"

# Build the Rust backend
build-backend:
    cargo build --release

# Build the frontend
build-frontend:
    cd frontend && npm run build

# Build both backend and frontend
build: build-frontend build-backend

# Run tests
test:
    cargo test

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Check compilation without building
check:
    cargo check

# Run clippy lints
lint:
    cargo clippy -- -D warnings

# Lint frontend
lint-frontend:
    cd frontend && npm run lint

# Format code
fmt:
    cargo fmt
    cd frontend && npm run fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt -- --check

# Clean build artifacts
clean:
    cargo clean
    rm -rf target/
    rm -rf frontend-dist/

# Build Docker image locally
docker-build:
    docker build -t dockru:latest -f ./docker/Dockerfile .

# Build Docker image for specific platform
docker-build-platform platform="linux/amd64":
    docker buildx build --platform {{ platform }} -t dockru:latest -f ./docker/Dockerfile .

# Build and push Docker image (update with your registry)
docker-push registry="localhost:5000":
    docker build --platform linux/amd64 -t {{ registry }}/dockru:latest -f ./docker/Dockerfile .
    docker push {{ registry }}/dockru:latest

# Deploy: build, push, and sync (customize for your deployment)
deploy registry="localhost:5000":
    docker build --platform linux/amd64 -t {{ registry }}/dockru:latest -f ./docker/Dockerfile .
    docker push {{ registry }}/dockru:latest
    @echo "Image pushed. Run your deployment command here."

# Run with docker-compose
docker-up:
    docker-compose -f docker/compose.yaml up

# Stop docker-compose services
docker-down:
    docker-compose -f docker/compose.yaml down

# Run the built binary directly
run:
    cargo run --release

# Run database migrations
migrate:
    @echo "Migrations are handled automatically on startup"

# Install dependencies
install:
    cargo fetch
    cd frontend && npm install

# Watch and run tests on file changes
test-watch:
    cargo watch -x test

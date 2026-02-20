############################################
# Build Stage
############################################
FROM rust:bookworm AS builder

WORKDIR /app

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy only Rust source (not frontend - built in separate stage)
# This ensures frontend-only changes don't invalidate the Rust build cache
COPY src ./src
COPY migrations ./migrations

# Build the real application
RUN cargo build --release

############################################
# Frontend Build Stage
############################################
FROM node:22-bookworm-slim AS frontend-builder

WORKDIR /app/frontend

# Copy frontend package files
COPY frontend/package.json frontend/package-lock.json ./

# Install dependencies
RUN npm ci

# Copy frontend source
COPY frontend/ ./

# Build frontend
RUN npm run build

############################################
# Runtime Stage
############################################
FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install --yes --no-install-recommends \
    ca-certificates \
    curl \
    gnupg \
    && install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && chmod a+r /etc/apt/keyrings/docker.gpg \
    && echo \
         "deb [arch="$(dpkg --print-architecture)" signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian \
         "$(. /etc/os-release && echo "$VERSION_CODENAME")" stable" | \
         tee /etc/apt/sources.list.d/docker.list > /dev/null \
    && apt-get update \
    && apt-get --yes --no-install-recommends install \
         docker-ce-cli \
         docker-compose-plugin \
    && rm -rf /var/lib/apt/lists/*

# Copy built binary from builder
COPY --from=builder /app/target/release/dockru /app/dockru

# Copy built frontend from frontend-builder (vite outputs to ../frontend-dist)
COPY --from=frontend-builder /app/frontend-dist /app/frontend-dist

# Create data directory
RUN mkdir -p ./data

# Disable io_uring for Node.js compatibility (if any Node.js components are used)
ENV UV_USE_IO_URING=0

# Expose application port
EXPOSE 5001

# Volume for persistent data
VOLUME /app/data

# Health check using the /health endpoint
HEALTHCHECK --interval=60s --timeout=30s --start-period=60s --retries=5 \
  CMD curl -f http://localhost:5001/health || exit 1

# Run the application
CMD ["/app/dockru"]

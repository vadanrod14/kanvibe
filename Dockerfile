# Use Rust official image as the base
FROM rust:1.88-slim AS builder

ARG RUST_VERSION=1.88

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    perl \
    make \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY backend/ ./backend/

# Set working directory to backend for build
WORKDIR /app/backend

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    libsqlite3-0 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the built binary
COPY --from=builder /app/backend/target/release/vibe-kanban /usr/local/bin/vibe-kanban

# Copy migrations
COPY backend/migrations /app/migrations

# Set working directory
WORKDIR /app

# Expose port
EXPOSE 8080

# Set environment variables
ENV PORT=8080
ENV DATABASE_URL=/app/data/db.sqlite
ENV RUST_LOG=info

# Create data directory
RUN mkdir -p /app/data

# Run the application
CMD ["vibe-kanban"]
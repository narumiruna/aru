set shell := ["bash", "-euo", "pipefail", "-c"]

# List available recipes.
default:
    @just --list

# Build aru in debug mode.
build:
    cargo build --all-targets --all-features

# Format Rust sources.
format:
    cargo fmt

# Verify formatting without changing files.
fmt-check:
    cargo fmt --check

# Run Clippy with warnings denied.
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Run the complete test suite.
test:
    cargo test --all-targets --all-features

# Run all repository quality gates.
check: fmt-check lint test

# Install aru to Cargo's default bin directory (~/.cargo/bin).
install:
    cargo install --path . --locked

# Install aru to ~/.local/bin.
install-local:
    cargo install --path . --root "$HOME/.local" --locked

# Remove Cargo build artifacts.
clean:
    cargo clean

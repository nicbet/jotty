# Makefile for jotty - TUI Jot App

.PHONY: all build debug check lint test clean run install help

# Default target
all: build

# Build release binary
build:
	cargo build --release

# Build debug binary
debug:
	cargo build

# Check code without building
check:
	cargo check

# Run linter
lint:
	cargo clippy -- -D warnings

# Run tests
test:
	cargo test

# Clean build artifacts
clean:
	cargo clean

# Run the application
run:
	cargo run

# Install binary to /usr/local/bin (requires sudo)
install: build
	sudo cp target/release/jotty /usr/local/bin/

# Show help
help:
	@echo "Available targets:"
	@echo "  all      - Build release binary (default)"
	@echo "  build    - Build release binary"
	@echo "  debug    - Build debug binary"
	@echo "  check    - Check code without building"
	@echo "  lint     - Run clippy linter"
	@echo "  test     - Run tests"
	@echo "  clean    - Clean build artifacts"
	@echo "  run      - Run the application"
	@echo "  install  - Install binary to /usr/local/bin"
	@echo "  help     - Show this help"

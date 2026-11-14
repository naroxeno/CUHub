# Cuhub build entrypoints. Run `just` for the list.

# Prefer the user's own cargo (~/.cargo/bin) over the system one.
export PATH := home_directory() + "/.cargo/bin:" + env_var("PATH")

default:
    @just --list

# type-check the workspace (cargo check for the crates)
check:
    nsh check

# run the Rust tests through nsh (adds app tests when they exist)
test:
    nsh test

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

lint: fmt-check clippy

# debug bundle to .nsh/out
build:
    nsh build

release:
    nsh build --release

# run the app on this system with hot reload
dev:
    nsh dev

clean:
    cargo clean
    rm -rf .nsh

ci: lint test

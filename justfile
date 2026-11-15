# Cuhub build entrypoints. Run `just` for the list.

# Prefer the user's own cargo (~/.cargo/bin) over the system one.
export PATH := home_directory() + "/.cargo/bin:" + env_var("PATH")

# Cross builds: package-windows runs here through cargo-xwin (cargo install
# --locked cargo-xwin, rustup target add x86_64-pc-windows-msvc), package-macos
# only on a Mac. Linux links go through wild via ~/.cargo/config.toml.

# Release tuning, shared by every packaged build. nsh compiles inside its own
# generated workspace (.nsh/rust), so the profile travels as cargo env
# overrides rather than a [profile.release] table:
# thin LTO + one codegen unit keeps most of fat LTO's code quality at a
# fraction of its link time; debug info is dropped but symbols stay, so
# backtraces keep their names. panic stays "unwind" on purpose: nsh catches
# component panics and shows them instead of aborting the app.
export CARGO_PROFILE_RELEASE_OPT_LEVEL := "3"
export CARGO_PROFILE_RELEASE_LTO := "thin"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS := "1"
export CARGO_PROFILE_RELEASE_STRIP := "debuginfo"

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

# installable package for this host in dist/<platform>/
package:
    rm -rf dist/{{os()}}
    nsh package --target {{os()}} --out dist/{{os()}}

# Linux directory and tarball in dist/linux; needs GTK 4.14+ and GStreamer at runtime.
package-linux:
    test "{{os()}}" = "linux" || { echo "package-linux builds on Linux only"; exit 1; }
    rm -rf dist/linux
    nsh package --target linux --out dist/linux

# Windows directory, .zip and Cuhub-setup.exe in dist/windows; cross-compiled.
package-windows:
    rm -rf dist/windows
    nsh package --target windows --out dist/windows

# macOS .app in dist/macos; needs a Mac, nsh links the AppKit runner against the SDK.
package-macos:
    test "{{os()}}" = "macos" || { echo "package-macos needs a Mac"; exit 1; }
    rm -rf dist/macos
    nsh package --target macos --out dist/macos

# run the app on this system with hot reload
dev:
    nsh dev

clean:
    cargo clean
    rm -rf .nsh

ci: lint test

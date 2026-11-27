# Cuprum build entrypoints. Run `just` for the list.

# Prefer the user's own cargo (~/.cargo/bin) over the system one.
user_home := if os() == "windows" { env_var("USERPROFILE") } else { home_directory() }
export PATH := user_home + "/.cargo/bin:" + user_home + "/.local/bin:" + env_var("PATH")

set windows-shell := ["niu", "-c"]
installer_shell := if os() == "windows" { "niu" } else { "sh" }

# Cross builds: package-windows runs here through cargo-xwin (cargo install
# --locked cargo-xwin, rustup target add x86_64-pc-windows-msvc), package-macos
# only on a Mac. Linux links go through wild via ~/.cargo/config.toml.

# Release tuning, shared by every packaged build. nsh compiles inside its own
# generated workspace (.nsh/rust), so the profile travels as cargo env
# overrides rather than a [profile.release] table:
# Thin LTO keeps link times manageable. Strip release symbols to shrink
# distributed binaries. Unwinding lets nsh display component panics.
export CARGO_PROFILE_RELEASE_OPT_LEVEL := "3"
export CARGO_PROFILE_RELEASE_LTO := "thin"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS := "1"
export CARGO_PROFILE_RELEASE_STRIP := "symbols"

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

# build the BepInEx bridge (needs dotnet 10 and the game's Managed DLLs)
build-bridge:
    dotnet build mods/cuprum-bridge/CuprumBridge.Plugin/CuprumBridge.Plugin.csproj -c Release

# platform-neutral bridge in dist/bridge; unzip into BepInEx/plugins
package-bridge: build-bridge
    rm -rf dist/bridge
    mkdir -p dist/bridge/CuprumBridge
    install -m 0644 mods/cuprum-bridge/CuprumBridge.Plugin/bin/Release/netstandard2.1/CuprumBridge.Plugin.dll dist/bridge/CuprumBridge/
    install -m 0644 mods/cuprum-bridge/CuprumBridge.Plugin/bin/Release/netstandard2.1/CuprumBridge.Core.dll dist/bridge/CuprumBridge/
    cd dist/bridge && zip -qr CuprumBridge.zip CuprumBridge

# installable package for this host in dist/<platform>/
package:
    rm -rf dist/{{os()}}
    nsh package --target {{os()}} --out dist/{{os()}}

# Linux directory and tarball in dist/linux; needs GTK 4.14+ at runtime.
package-linux:
    test "{{os()}}" = "linux" || { echo "package-linux builds on Linux only"; exit 1; }
    rm -rf dist/linux
    nsh package --target linux --out dist/linux

# Linux AppImage in dist/appimage; requires appimagetool.
package-appimage:
    test "{{os()}}" = "linux" || { echo "package-appimage builds on Linux only"; exit 1; }
    nsh package --target linux --format appimage --out dist/appimage

# Windows directory, .zip and Cuprum-setup.exe in dist/windows; cross-compiled.
package-windows:
    rm -rf dist/windows
    nsh package --target windows --out dist/windows
    {{installer_shell}} ./scripts/windows-installer.sh

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

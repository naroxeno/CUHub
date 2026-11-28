<p align="center"><img src="crates/app/assets/branding/cuprum-package.png" width="128" height="128" alt="Cuprum pixel-art copper ore icon"></p>

<h1 align="center">Cuprum</h1>

<p align="center">
  An unofficial mod manager and launcher for <a href="https://store.steampowered.com/app/4576490/">Casualties: Unknown</a>.
</p>

<p align="center">
  <strong>Alpha</strong> · Windows / Linux · <a href="README.zh-CN.md">简体中文</a>
</p>

## Features

| Feature | What you can do |
| --- | --- |
| **Mod management** | Import ZIP, 7z, RAR, or DLL files. Preview installs and updates, check dependencies, and enable or disable mods. |
| **Mod discovery** | Search GitHub and Nexus Mods in one place. Use your browser or the authorized Nexus API to download Nexus mods. |
| **Profiles** | Keep multiple mod versions locally and switch between loadouts. |
| **BepInEx & Steam** | Install or repair BepInEx 5 LTS or experimental 6 Mono, then launch through Steam. |
| **Multiplayer · Experimental** | Host or join EasyTier rooms with a room code. |

## Quick start

1. In **Settings**, select your game folder and install BepInEx if needed.
2. Browse **Remote Mods** or import a local file. Review the install in the **Task Center** before applying.

For multiplayer, install **Casualties Together** and its dependencies, then open **Multiplayer**. The host defaults to port **7790**. Guests can leave the host port blank and paste the address Cuprum shows into the game. If the address changes, rejoin with the new one. Two-machine play and the optional Bridge are still pending in-game testing.

### Upgrading an older test build

Before launching the new build, close the old app and the game. From a source checkout, run `python3 scripts/migrate-app-data.py "<old data folder>"` (`py -3` on Windows). Pass the folder containing `Settings.json`. The script moves the folder to a sibling named `Cuprum` and leaves a directory link so saved mod paths still work. An existing destination must be moved aside manually before retrying.

If you use the Bridge, move the old Bridge folder out of BepInEx's plugins directory before installing the new package. Turn automatic in-game hosting and joining back on if you used it before. Everyone in a multiplayer room should upgrade and use a newly created room.

## Build from source

Install stable Rust, [nsh](https://nsh.dev) 0.7.1, and `just`. On Linux, use Clang to build and GTK 4.14+ to run the app.

```sh
git clone https://github.com/naroxeno/Cuprum.git
cd Cuprum
just dev
```

Use `just package-linux` for the Linux release archive and `just package-windows` for the Windows ZIP and setup executable. The Windows recipe adds the license and documentation and rebuilds the installer.

To add an extracted Linux package to your application menu, run `bash scripts/register-linux-launcher.sh "<directory containing Cuprum>"` from a source checkout with Python 3 installed. Rerun it after moving the package.

On Linux, `just package-appimage` writes the AppImage to `dist/appimage/`. Put `appimagetool` on `PATH` or set `NSH_APPIMAGETOOL` to its path. The AppImage uses the target system's GTK 4.14+.

Run `just check` and `just test` for validation. See the [justfile](justfile) for platform-specific builds. Package the optional Bridge separately with `just package-bridge`. Build and install steps are in the [Bridge guide](mods/cuprum-bridge/README.md).

## Project

[Features](docs/FEATURES.md) · [Roadmap](docs/ROADMAP.md) · [Test plan](docs/TEST-PLAN.md)

[AGPL-3.0-or-later](LICENSE) · [Third-party notices](docs/third-party/)

API keys stay on your machine. Cuprum collects no telemetry.

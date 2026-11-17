# Cuhub

**An unofficial mod manager and launcher for [Casualties: Unknown](https://store.steampowered.com/app/4576490/).** Built in Rust with [nsh](https://nsh.dev), in the game's pixel style.

[English](README.md) | [简体中文](README.zh-CN.md)

## What it does

- **Manage what is installed.** Scan `BepInEx/Plugins`, enable or disable mods, remove them safely, and see available updates.
- **Install without surprises.** ZIP, 7z, RAR, or a bare DLL: Cuhub stages the archive, shows the exact plan, backs up what it replaces, and can roll back. Declared dependencies are checked before installing or updating.
- **Set up BepInEx.** Install or repair BepInEx 5 LTS or 6 Mono, with SHA-256 verification.
- **Find mods.** Browse the community Nexus catalogue and GitHub release sources, or install from a local manifest. Nexus downloads only use the official API with your own key.
- **Keep a reusable library.** Mods live once in a content store and are linked into `Plugins`; profiles switch the active set.
- **Play together (experimental).** A built-in EasyTier virtual LAN with room codes — no account and no self-hosted server. Two-machine testing is still pending.
- **Look the part.** Pixel UI, 1x/2x/3x integer scaling, English / 简体中文 / 日本語.

## Status

0.1.0 alpha. Windows is the main target; Linux is supported. There are no packaged releases yet — build from source. On-disk formats and UI may still change.

## Build

Requires Rust (stable), [nsh](https://nsh.dev) 0.5.1, and `just`. Linux also needs clang to build, plus GTK 4.14+ and GStreamer at runtime.

```sh
git clone https://github.com/naroxeno/CUHub.git
cd CUHub
just check
just test
just dev          # run with hot reload
just package      # package for the host, into dist/
```

`just package-windows`, `package-linux`, and `package-macos` build specific targets.

## Docs

- [Architecture](docs/ARCHITECTURE.md)
- [Feature list](docs/FEATURES.md)
- [Handover notes](docs/HANDOVER.md)

## License

[AGPL-3.0-or-later](LICENSE). Cuhub uses third-party work; attribution and license notes live in [docs/third-party](docs/third-party/).

## Disclaimer

Cuhub is unofficial and is not affiliated with or endorsed by the developer of Casualties: Unknown, Nexus Mods, or BepInEx. It collects no telemetry, and API keys stay on your machine.

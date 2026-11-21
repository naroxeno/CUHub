# CUHub

An unofficial desktop mod manager and launcher for [Casualties: Unknown](https://store.steampowered.com/app/4576490/). Built in Rust with [nsh](https://nsh.dev) for Windows and Linux.

[简体中文](README.zh-CN.md)

## What you can do

- **Manage mods.** Scan your BepInEx installation, enable or disable mods, and prepare updates. Install ZIP, 7z, RAR, or standalone DLL files with a deployment preview and dependency checks.
- **Browse GitHub and Nexus together.** Search both catalogues, filter by source or installation status, and choose a source for each install. Nexus supports browser downloads and authorized downloads through the official API.
- **Switch profiles.** Keep mod versions in a local Store and apply a profile to change the active set. CUHub checks for unmanaged files in the way before applying a profile. A deployment interrupted by a disk error can leave partial changes.
- **Set up BepInEx and launch through Steam.** Install or repair BepInEx 5 LTS or experimental BepInEx 6 Mono using verified downloads.
- **Play together experimentally.** Create an EasyTier room and share its C1 code. Guests can discover the host's port automatically and copy a local forwarding address into Casualties Together. The optional CUHub Bridge can also request in-game hosting and joining.

## Getting started

1. Select your game installation in **Settings**. Install BepInEx there if needed.
2. Browse **Remote Mods**, or import your own archive. Review the source, version, and target in the task center before applying.
3. For multiplayer, install Casualties Together and its dependencies, then open **Multiplayer**. The host uses port **7790** by default. Guests can leave the host-port field blank. After a host endpoint change, rejoin in the game using the latest address shown in CUHub.

CUHub is in alpha. Multiplayer forwarding has been exercised locally with EasyTier 2.6.4. Full two-machine play and the optional Bridge still need real-game testing.

## Build from source

Install stable Rust, [nsh](https://nsh.dev) 0.5.1, and `just`. On Linux, install Clang for builds and GTK 4.14+ and GStreamer for the desktop app.

```sh
git clone https://github.com/naroxeno/CUHub.git
cd CUHub
just check
just test
just dev
just package
```

`just package` builds for the current platform. See the [justfile](justfile) for platform-specific packaging. The optional bridge is a separate archive built with `just package-bridge`. Its build requires the .NET SDK and the game's managed assemblies. Place `CuhubBridge.zip` next to the app executable, then use **Local Mods → CUHub Bridge → Install**. See the [Bridge guide](mods/cuhub-bridge/README.md).

## Project information

- [Feature list](docs/FEATURES.md) · [Architecture](docs/ARCHITECTURE.md) · [Roadmap](docs/ROADMAP.md)
- [Test plan](docs/TEST-PLAN.md) · [Handover notes](docs/HANDOVER.md)
- [AGPL-3.0-or-later](LICENSE) · [Third-party notices](docs/third-party/)

CUHub is an independent community project. API keys stay on your machine, and CUHub collects no telemetry.

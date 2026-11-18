# CuhubBridge

cuhub 与游戏进程之间的桥，以及给其他 CU mod 用的接入点。协议契约在 `docs/bridge/protocol.md`，这里说工程结构、构建与部署。

## 实现状态（MVP，2026-11-19，实验性，不发布）

游戏侧已打通最小闭环，`dotnet build -c Release` 通过且无警告；尚未在真机加载验证。

- `CuhubBridge.Core`（netstandard2.0）：协议帧（u32 LE 长度 + UTF-8 JSON，1 MiB 上限）、`hello`/`req`/`res`/`evt` 数据结构、rendezvous 文件校验、TCP 连接与读写队列、`Api` 契约。不依赖 BepInEx，但引用游戏自带的 `Newtonsoft.Json.dll`。
- `CuhubBridge.Plugin`（netstandard2.1）：`[BepInPlugin("dev.cuhub.bridge", "CuhubBridge", "0.1.0")]` 探针。`Update()` 主线程泵负责发现 rendezvous、连接宿主、`hello`、10 s 心跳（连续 3 次未回重连）与 `cmd.invoke` 分发。
- 命令：`mp.state` 反射 `CUCoreLib.Networking.MultiplayerApi`；`mp.host` / `mp.join` 反射 `MultiplayerBridge.TryConfigureLocalIdentity` + `TryStartLocalQuickTestHost/Client`；命令表外的 id 回 `unknown_command`，命令表内但本 build 未实现的（`mp.leave` / `mp.kick` / `save.*` / `skin.*`）回 `unavailable` 并写明未实现。缺 CUCoreLib、方法缺失、返回 false 或抛异常时回 `unavailable` 并带原因。
- 线程：`BridgeClient` 的读线程只解帧入队，写线程只发队列，都不碰 Unity；所有游戏与 CUCoreLib 调用都在 `Update()` 主线程；结果进写队列由写线程发出。

尚未实现：`game.state` / `mods.loaded` 推送、`ask` 权限的用户确认、`save.*` / `skin.*`、`mp.leave` / `mp.kick`（CUCoreLib 没有公开 API）。`mp.host` / `mp.join` 的 QuickTest 语义未在真机验证。宿主侧（Rust）MVP 已实现监听、握手、命令表、权限与会话，见 `crates/core/src/bridge/`。

## 目录

- `CuhubBridge.Core/` — `Contract.cs`（协议常量与命令表）、`Messages.cs`、`FrameCodec.cs`、`Rendezvous.cs`、`BridgeClient.cs`。
- `CuhubBridge.Plugin/` — `BridgePlugin.cs`、`BridgeSession.cs`（连接与协议状态）、`CommandRouter.cs`、`MpBackend.cs`（全部反射集中在 `MpBackend.cs`，以后拆 `CuhubBridge.Mp` 时直接搬走）。

## 构建

    dotnet build mods/cuhub-bridge/CuhubBridge.Core/CuhubBridge.Core.csproj -c Release
    dotnet build mods/cuhub-bridge/CuhubBridge.Plugin/CuhubBridge.Plugin.csproj -c Release

默认游戏目录是 `$HOME/.steam/steam/steamapps/common/Casualties Unknown`；不在默认位置时用环境变量或属性覆盖：

    CUHUB_GAME_DIR=/path/to/game dotnet build mods/cuhub-bridge/CuhubBridge.Plugin/CuhubBridge.Plugin.csproj -c Release
    dotnet build mods/cuhub-bridge/CuhubBridge.Plugin/CuhubBridge.Plugin.csproj -c Release -p:CuhubGameDir=/path/to/game

`CuhubBridge.Core` 是 `netstandard2.0`。`CuhubBridge.Plugin` 是 `netstandard2.1`：游戏 Unity 2022.3.62f3 的 `UnityEngine.CoreModule.dll` 引用 netstandard 2.1，插件用 2.0 会在编译期报 CS1705。Unity 2022 的 Mono 运行时支持 2.1；真机上能否被 BepInEx 5.4.23.6 正常加载仍需验证。

编译只需要 dotnet SDK 与游戏目录里的元数据，不需要 Wine/Proton。运行与调试需要 Proton/Wine：游戏是 Windows x86_64 Unity Mono 程序，插件只在游戏自带 Mono 里被加载。

`mods/**/bin` 与 `mods/**/obj` 已被 `.gitignore` 忽略，构建产物不入库。

## 部署（待真机验证）

把 `CuhubBridge.Plugin.dll` 与 `CuhubBridge.Core.dll` 一起放到 `BepInEx/plugins/CuhubBridge/`。Core 不要复制到多个 mod 目录——桥的前提是进程内只有一份、只有一条连接。

`bridge.json` 由宿主写到游戏 prefix 的 `drive_c/ProgramData/CasualtiesHub/bridge.json`；插件只读，不创建文件，也不改游戏目录。宿主缺失或文件无效时插件正常加载并保持静默重试。

## 被其他 mod 引用

- 只引用 `CuhubBridge.Core`，设 `<Private>false</Private>`，不要复制到自己的 plugins 目录。
- 现在还没有 `Api.EnsureStarted()` 或命令注册接口；`Api.Implemented = true` 只表示插件闭环已接通，不代表第三方接入面已定。可用的是协议常量、`BridgeMessage`、`FrameCodec`、`Rendezvous` 这类数据结构。
- 引用 Core 的 mod 需要游戏的 `Newtonsoft.Json.dll`（游戏自带；编译时 HintPath 指向 Managed，运行时由游戏加载）。
- 第三方命令注册、策略与参数 schema 等接入面定了再补进 `docs/bridge/protocol.md`。

## 版本

- 程序集版本与 `Api.ContractVersion` 分开：前者跟发布走，后者只在 API 破坏性变更时 +1。
- 线上协议版本单独记在 `docs/bridge/protocol.md`，与程序集版本解耦。

# CuhubBridge

cuhub 与游戏进程之间的桥，以及给其他 CU mod 用的接入点。协议契约在 `docs/bridge/protocol.md`，这里只说工程怎么长、怎么装、怎么被引用。

## 实现状态（实验性，不发布）

当前只有 `CuhubBridge.Core` 的协议常量与 `Api.Implemented = false`：没有传输、没有监听、没有命令。`CuhubBridge.Plugin` 还没建——它要引用 BepInEx 5 与游戏的 `Managed` DLL（`Assembly-CSharp.dll`、`UnityEngine.dll` 等），本机没有，所以 `dotnet build` 只跑零依赖的 Core。宿主侧同理，只有 `crates/core/src/bridge/` 的帧与发现文件。

联机状态要经 CUCoreLib 的 `MultiplayerApi`（`CUCoreLib.Networking`）读，等于插件对 CUCoreLib 硬依赖；这件事和命令 id 是否加 `cuhub.` 前缀一起，等真机接入再定。

编译不需要 Wine/Proton：Roslyn 只读元数据，Linux 上引用 Windows PE 程序集完全可行。需要 Wine 的是运行与调试——游戏是 Windows x86_64 的 Unity Mono 程序，BepInEx 5 跑在游戏自带的 Mono 上，插件只能在 Proton/Wine 里被真实加载。

## 目录

- `CuhubBridge.Core/` — 传输、帧、发现、命令注册。**不依赖 BepInEx**，装到 `BepInEx/core/CuhubBridge.Core.dll`，全进程唯一一份。
- `CuhubBridge.Plugin/` — `BepInPlugin` 入口与主线程泵，装到 `BepInEx/plugins/CuhubBridge.Plugin.dll`。

拆两个程序集不是洁癖：合成一个的话，每个消费 mod 都会带一份副本，进程里就有 N 个实例、N 条连接，单连接这个前提直接没了。

## 构建

    just bridge-build      # dotnet build -c Release
    just bridge-package    # 组装可安装的 core/ + plugins/ 目录

TFM 默认 `netstandard2.0`（BepInEx 5 与 6-Mono 都吃）；如果消费 mod 已经是 `net6.0`，Core 抬到 `netstandard2.1`。

## 被其他 mod 引用

- 只引用 `CuhubBridge.Core`，设 `<Private>false</Private>`，不要复制到自己的 plugins 目录。
- 启动时先 `CuhubBridge.Api.EnsureStarted()`，再注册命令；它可重入，不要自己起线程。
- 命令注册带策略（`read` / `control` / `mutate`）与参数 schema，宿主只认注册过的 id。
- handler 一律在游戏主线程执行；桥自己的线程不碰 Unity API。
- 桥不在时接口必须可调用且返回“未连接”，消费 mod 不需要为它写分支。

## 版本

- 程序集版本与 `Api.ContractVersion` 分开：前者跟发布走，后者只在 API 破坏性变更时 +1。
- 线上协议版本单独记在 `docs/bridge/protocol.md`，与程序集版本解耦。

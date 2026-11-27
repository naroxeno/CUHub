# C# 侧插件：API 研究与实现建议

研究时间 2026-11-18。源码依据：CUCoreLib 仓库（本地 /tmp/cucorelib）、Casualties Together 反编译参考（/tmp/ct-decomp，仅阅读，不入库）、CT 的 FOR-NERDS.txt。这里只给结论与约束，代码另开任务。

## 结论

1. **CT 没有给第三方 mod 的网络 API。** 它的 `KrokoshaCasualtiesMultiplayerAPI` 命名空间只有 GamemodeManager / GamemodeBase / BattleRoyale，是玩法模式，不是联机接口。联机状态只能反射读。
2. **CUCoreLib 已经把这层反射做完了**，门面是 `CUCoreLib.Networking.MultiplayerApi`（游戏内 mod 之间通信，走 CT 的 LiteNetLib 自定义消息通道）。它不是本机 IPC，但它是游戏侧唯一稳定的状态/控制入口。插件应复用它，别自己重写反射。
3. **本机 IPC 用 TCP loopback 的固定端口 `127.0.0.1:41297`**（协议已定）。不要改用命名管道：Linux 玩家走 Proton，Wine 的 `\\.\pipe\` 由 wineserver 管，原生 Rust 进程连不进去；Wine 里的 TCP 是真实 loopback，能通三端。也不要回到发现文件：`%ProgramData%`、prefix、bottle 三种推导都不可靠，固定端口把发现这一步整个省掉。

## 实现状态（MVP，2026-11-19）

已按最小闭环落地两个程序集，`dotnet build -c Release` 通过且无警告，尚未真机加载：

- `CuprumBridge.Core`（netstandard2.0）：帧编解码、`hello`/`req`/`res`/`evt`、固定端点与共享 token、TCP 读写线程与队列、`Api` 契约。引用游戏自带 `Newtonsoft.Json.dll`，不引用 BepInEx/Unity。
- `CuprumBridge.Plugin`（netstandard2.1）：`dev.cuprum.bridge` 探针，`Update()` 里连固定端点、握手、心跳与 `cmd.invoke` 分发；连不上就退避重连，不影响游戏。`BridgeSession.cs` / `CommandRouter.cs` / `MpBackend.cs` 分文件；MVP 未拆 `Mp` / `Transport` 程序集。
- TFM 差异：游戏 Unity 2022.3.62f3 的 `UnityEngine.CoreModule.dll` 是 netstandard 2.1，插件用 2.0 会报 CS1705，所以插件取 2.1；Core 保持 2.0。
- 线程：`BridgeClient` 读线程只解帧入队，写线程只发队列；Unity / CUCoreLib 调用只在 `Update()` 主线程。
- 命令：`mp.state` 读 `MultiplayerApi` 的 `IsAvailable/IsRunning/IsClient/IsServer/IsHost/BridgeReceiversInstalled/KrokMpVersion`，并在 `running && server` 时反射 `Together.Net.Transport` → `TransportLiteNetLib.port` 给出扁平 `port`（客人、Steam、读不到都是 `null`）；`mp.host`/`mp.join` 在带 `name` 时先调 `TryConfigureLocalIdentity`，再调 `TryStartLocalQuickTestHost/Client`，成功后回读 `Together.Net` 的角色与端口，回 `{started, host, address, port}`；命令表外的 id 回 `unknown_command`，命令表内但本 build 未实现的（`mp.leave` / `mp.kick` / `save.*` / `skin.*`）回 `unavailable`，reason 写明 "command declared but not implemented in this build"。
- 失败分三类：参数问题（`address` 缺失/端口解析失败/端口 0/`name` 少于 3 字符）回 `invalid_params`；端口被占用回 `busy`；缺 CUCoreLib/Together、方法缺失、返回 `false`、回读不一致、反射抛异常回 `unavailable` 并带原因。CT 的失败原因只有本地化 `multiplayerStatusMessage`，只拼进 message 当线索，不据此分支。`CommandRouter.Execute` 外层有 catch-all，handler 异常不会冲进 Unity `Update()`。`mp.leave`、`mp.kick` 没有公开 API，未实现。
- 未实现：`game.state` / `mods.loaded` 推送、权限、`save.*` / `skin.*`；入站 `evt` 全部忽略；`ping` 双向，插件每 10 s 发 `ping` 并回宿主的 `ping`。
- 自验：临时 console harness（在 `/tmp`，未入库）覆盖帧往返/半截/超大、消息解析、固定端点与共享 token、回环 TCP hello/res，21 项全过；未覆盖真机加载与 CUCoreLib 反射。

## CUCoreLib 实况

- 插件 GUID `net.cucorelib`，VERSION 1.1，`[BepInDependency("CasualtiesMP", SoftDependency)]`（对 CT 是软依赖）。编译要游戏 Managed DLL，DLL 不入库。
- `MultiplayerApi` 公开面（与桥相关的）：
  - 状态：`IsAvailable / IsRunning / IsClient / IsServer / IsHost`，`BridgeReceiversInstalled`，`KrokMpVersion`。
  - 网络：`SendToServer / RequestServer(channel, payload, onResponse) / SendToClient / Broadcast / BroadcastEverywhere / BroadcastToPeer`，handler 注册 `RegisterServerHandler(channel, Func<JToken,JToken>)`、`RegisterClientHandler(channel, Action<JToken>)`、`RegisterHandler`。
  - 玩法读写：`TryGetLocalBody(out Body)`、`Apply/ApplyAbsolute(Body|Limb, field, value|delta)`，字段限定 public 实例字段且只能是 float/bool，名字缓存。
  - 状态同步：`RegisterSyncModule(key, capture, apply)`、`CaptureSnapshot / ApplySnapshot / BroadcastSnapshot`、`ReportLoadingProgress`。
- `MultiplayerBridge`（public static，1150 行）：除上面的转发外，还有 `TryConfigureLocalIdentity(username, address)`（写 `Together.UIMainMenu.USERINPUT_NAME` / `USERINPUT_IPPORT`）、`TryStartLocalQuickTestHost(address)` / `TryStartLocalQuickTestClient(address)`（语义未验证：名字里的 QuickTest 可能只是测试快捷入口，不一定等价于 UI 的 Host / Join）、`TryAnnounceGameStart()`、`Initialize(Harmony)`。
- 反射目标（CT v7.x）：`Together.Multiplayer`、`Together.Net`（static bool 属性 IsRunning/IsClient/IsServer/IsHost/IsConnected）、`Together.ServerMain`（`AllClientIds` / `AllClientIdsExceptHost` 两个 static 成员）、`Together.ClientMain`、`Together.TransportLiteNetLib`、`Together.UIMainMenu`、`Together.SavesystemPatch`（`savedatapathreplacement` / `mpsavefolder`）、`Together.MyLiteNetLibExtensions`。
- 命名空间正在从 `KrokoshaCasualtiesMP` 迁到 `Together`（v5→v7），CUCoreLib 维护了一张类型/成员别名表来兼容旧名。这就是不建议自研反射的理由。
- 主线程调度：`CUCoreUtils.CallWhen(Func<bool> condition, Action action, float retry)`、`CUCoreUtils.StartCoroutine(IEnumerator)`（public，但必须在主线程调用）。

## CUCoreLib 核对（2026-11-20）

按 GitHub `jimmyking9999999/CUCoreLib` 的 `main` 源码（`Networking/MultiplayerApi.cs`、`Networking/MultiplayerBridge.cs`）逐项核对 README 与 `MpBackend.cs` 的反射目标，结论：名字与参数形状一致，不需要改反射。

- `MultiplayerApi` 的 `IsAvailable` / `IsRunning` / `IsClient` / `IsServer` / `IsHost` 都是 public static bool 属性；`BridgeReceiversInstalled` 是 public static bool；`KrokMpVersion` 是 `System.Version`（不是 string），`MpBackend` 用 `ToString()`，得到 `1.2.3.0` 形状。
- `MultiplayerBridge.TryConfigureLocalIdentity(string username, string address)`、`TryStartLocalQuickTestHost(string address)`、`TryStartLocalQuickTestClient(string address)` 的签名与调用顺序一致；`address` 的实际语义是 `host:port`，空串时 CUCoreLib 回退 `localhost:7790`。
- 程序集名是 `CUCoreLib`（`net.cucorelib` 是 BepInEx 插件 GUID）。`MpBackend` 的 `AssemblyNames` 把 GUID 当后备名字不会命中，但先扫 AppDomain，实际无影响。
- 风险：旧版 CUCoreLib 是否都带 `KrokMpVersion` 未逐一核对；缺该属性时反射读回 null，`mp.state` 的 `krokmp_version` 为 null，状态卡显示 unknown，不抛异常。

## Together 反射：实际端口（2026-11-21）

源码依据 `ref/casualties-together`（程序集名 `CasualtiesMP`，命名空间 `Together`）。插件对 Together 仍是全反射、零引用。

- `Together.Net`（`ScavPrototypeMP/Net/Net.cs`）：`IsRunning` / `IsClient` / `IsServer` / `IsHost` 是 public static 属性（:210-215）；`Transport` 是 public static 属性（:221，`internal set` 只限制写，不影响读）。
- `Together.TransportLiteNetLib`（:948）：`ip` / `port` 是 public readonly 字段（:950-951），`port` 为 `ushort`。`TransportSteam` 没有端口字段。
- 房主侧 `NetManager.Start(port)` 直接绑这个端口（:1042）；客人侧同一个字段表示目标端口（:1030/:1036）。所以读取必须用 `IsServer` 门控——只在房主侧把 `port` 当监听端口，客人侧不监听本机端口，`mp.state.port` 恒 null。
- 读取顺序（`MpBackend.ProbePort`）：Together 缺失或抛异常 → `Unavailable`；`IsRunning` 为假或 `IsServer` 为假 → `NotHosting`；`Transport` 为 null、`port` 字段缺失/类型不是 `ushort`/`port == 0` → `Unavailable`；其余 → `Ready(port)`。类型查找沿用 `FindType`：先扫 `AppDomain.CurrentDomain.GetAssemblies()`，再 `Assembly.Load("CasualtiesMP")` 兜底；**失败不缓存**，下一条命令重试，因此装/卸 Together 后不需要重启游戏。
- 失败不抛：所有反射包在 try/catch 里，异常转成 `unavailable` + 可读 message。`mp.state` 把 `Unavailable` 映射成 `port: null`（快照不该因探测失败而失败），`mp.host`/`mp.join` 则把 `Unavailable` 映射成 `unavailable` 错误。
- 端口解析照抄 `Net.TryParseIPPORT`（:720-807）的既有怪癖，见 `docs/bridge/protocol.md` §6.2：无点号主机名带端口会被 CT 当成 IPv6、端口回落 7790。插件不修正这个行为，保证回给宿主的端口与游戏实际绑定的端口一致。
- 端口占用探测（`MpBackend.IsPortBusy`）：UDP 试绑 `IPAddress.Any`，IPv6 可用时再试 `IPv6Any`，只有 `SocketError.AddressAlreadyInUse` 算占用，其它错误一律不算（不能因为权限或协议栈问题把一次正常开房拦下）。

## CT 实况（能用的非 API 手段）

- 程序集名 `KrokoshaCasualtiesMP`，插件 GUID `CasualtiesMP`。联机走 UDP，默认 7790。
- 启动参数：`--mp-startserver` / `--mp-starthost` / `--mp-startclient` / `--mp-ip-port 127.0.0.1:7790` / `--mp-setname` / `--mp-setpass` / `--mp-setrule k v` / `--mp-runcommand "..."` / `--mp-verbose` / `--mp-immidiate-start`。
- 控制台命令（可解析文本输出）：`kick <name|id:N>`、`heal @a`、`maxplayers`、`currentrules`、`admin`、`adminpriv`、`give`。玩家宏 `@m @a @r @c`。
- 存档落在 `%LocalAppData%Low/Orsoniks/CasualtiesUnknown/mp_save/`，另有 `mp_settings.json`、`mp_banlist.json`、`mp_rule_presets.json`。

## 建议的实现分层

```
Bridge.Plugin (BepInEx BaseUnityPlugin)
  └─ 主线程泵：ConcurrentQueue<Envelope> ← Update()
     └─ CommandRouter：cmd id → handler，权限二次校验
        ├─ MpBackend（接口）：Snapshot / Host / Join / Kick / SaveScope
        │    ├─ CucoreLibBackend（反射 net.cucorelib，优先）
        │    └─ NativeBackend（反射 Together.*，兜底）
        └─ Bridge.Transport：TCP client + 帧编解码 + 读写线程 + 心跳
```

MVP 先落地为 Plugin + Core 两个程序集：上图的 `MpBackend` 是 Plugin 内的 `MpBackend.cs`，Transport 的 TCP 与读写线程在 Core 的 `BridgeClient.cs`；等有第二个调用方再拆程序集。

- **依赖策略：软依赖 + 全反射，不项目引用 CUCoreLib.dll。** BepInEx 的 `SoftDependency` 只管加载顺序，直接引用类型在缺库时仍是 TypeLoadException，插件会整体挂掉。反射调用缓存 MethodInfo，开销可忽略（每命令几次）。
- **线程：读线程只解帧和入队，任何 Unity / CUCoreLib 调用都在 Update() 里跑。** 响应写入走独立写队列，不阻塞主线程。
- **状态推送：1 s 轮询 + diff。** `Together.Net` 的 static bool 属性读取很便宜，不必 Harmony patch 联机事件（patch 别人的 patch 容易打架）。
- **宿主不碰游戏文件。** save.* 只回路径与 scope，删除/覆盖由游戏侧确认后做，或干脆交给 Cuprum 的 service 层走用户确认。

## 命令落点建议

| 协议命令 | 落点 | 备注 |
|---|---|---|
| mp.state | 反射 `MultiplayerApi` 的布尔与 `KrokMpVersion`；`running && server` 时再反射 `Together.Net.Transport` → `TransportLiteNetLib.port`（已实现） | 玩家列表仍未接（`Together.ServerMain.AllClientIds`，待真机）；`port` 只在房主侧非空 |
| mp.host / mp.join | `TryConfigureLocalIdentity` + `TryStartLocalQuickTestHost/Client`，成功后回读 `Together.Net` 确认并回 `address`/`port` | MVP 已按此反射接通；`address` 解析照 `Net.TryParseIPPORT`；QuickTest 等价性待真机确认 |
| mp.kick | 走 CT 控制台 `kick` | 需先确认能执行控制台命令（ConsoleCommandRegistry / ConsolePatch） |
| save.* | `GetKrokMpSaveScope`（internal，反射） | 只读判定 |
| skin.* | 本地文件 | 与联机无关 |

## 风险

- CT 更新可能改 `Together.*` 成员；CUCoreLib 不跟就一起碎。宿主已把协议不符与最近握手失败写进状态栏，联机页显示 `KrokMpVersion`；普通 `mp.state` 调用的 `KrokMpVersion` 缺失仍按 unknown 显示。
- `Together.Net.Transport` 或 `TransportLiteNetLib.port` 改名/改类型时：`mp.state.port` 变 null，`mp.host`/`mp.join` 回 `unavailable`（message 带 "has no public ushort port field"），不崩也不假装成功。真机日志出现这句就说明 CT 改了成员，改反射即可，协议不用动。
- `TryStartLocalQuickTestHost/Client` 名字含 QuickTest，语义可能是测试用快捷入口而非正式 Host；真机要先验证它是否等价于 UI 按钮。
- 反射到 `Body`/`Limb` 的写法在无 CUCoreLib 时必须自己维护字段白名单，容易越权改数值 → 默认只在 CucoreLibBackend 下开放玩法写，NativeBackend 只读。
- 控制台命令执行如果靠 Harmony 注入，属于高风险区，需单独评审。

## 待真机确认

1. `TryStartLocalQuickTestHost/Client` 与 UI 的 Host/Join 是否等价。
2. 玩家列表（id + 名字）的稳定来源。
3. 控制台命令的程序化执行入口。
4. `--mp-startserver` 的 dedicated server 是否真的无 UI，可被 Cuprum 当主机拉起。
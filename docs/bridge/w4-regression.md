# W4 回归：技术事实与端口检测机制

2026-11-21。本文件是 W4「游戏房间端口自动检测」的调研底稿，只写**已核实的事实**与**机制结论**，不写实现计划、不写代码改动。范围限定在两件事：游戏内联机端口（Together / QuickTest）到底在哪、以及 Cuprum 怎么在不改 Together、不读 `/proc` 的前提下把它读出来。

证据只有两类，所有结论都带其中一种：

- **源码**：`ref/casualties-together`（与安装版 Together 5.5.4 同源，UI 文案与日志实测一致）、`ref/CUCoreLib`（1.1）、本仓库 `mods/cuprum-bridge`、`crates/`。
- **真机**：本机 Steam AppID 4576490（Windows 版，DW-Proton），BepInEx 5.4.23.6（Unity 2022.3.62），日志 `<game>/BepInEx/LogOutput.log`。命令见 §1.8。

---

## 1. 技术事实

### 1.1 端口常量与菜单默认值

| 事实 | 证据 |
|---|---|
| 默认端口 7790 | `ref/casualties-together/ScavPrototypeMP/Net/Net.cs:249` `public const ushort DEFAULT_PORT = 7790;` |
| 菜单输入框默认 `127.0.0.1:7790` | `ScavPrototypeMP/UI/UIMainMenu.cs:1424` `USERINPUT_IPPORT = Net.IP_LOCALHOST_IPv4 + ":" + Net.DEFAULT_PORT.ToString()`；真机字段实测同值（§1.8 ②） |
| 端口来自输入框，不是常量 | 解析在 `Net.cs:720-807`：无 `:` 时回落 `DEFAULT_PORT`（`:798`），端口段 `ushort.TryParse` 失败即返回 false（`:800-802`）。真机把输入改成 `127.0.0.1:24444` 后监听端口随之改变（§1.8 ③） |
| 输入支持 IPv4 / 域名 / IPv6 方括号写法 | `Net.cs:726-736` 注释列出的 8 种形态 |

### 1.2 两种 transport，只有一个有本地端口

`Together.Net` 是静态类（`Net.cs:22` namespace `Together`，`:203` `public static class Net`），通过 `public static NetTransportBase Transport { get; internal set; }`（`Net.cs:221`）指向当前 transport。实现有两套，**互斥**：

| transport | 位置 | 本地监听端口 |
|---|---|---|
| `TransportLiteNetLib`（直连 IP） | `Net.cs:948` `public class TransportLiteNetLib : NetTransportBase, INetEventListener` | 有。`public readonly string ip`（`:950`）、`public readonly ushort port`（`:951`），构造函数把入参写进这两个字段（`:956-958`） |
| `TransportSteamworks`（Steam 大厅） | `ScavPrototypeMP/Net/SteamworksNet.cs:1982` `public class TransportSteamworks : NetTransportBase` | **没有**。类里只有 `SteamIDToNetPlayerDict`（`:2648`）等字段，无 `port` 成员；Steam 主机走中继/大厅，没有本地 UDP 端口概念 |

真机验证了这条区分：菜单「使用 Steam」勾选时点「创建大厅」，`ss -lunp` **没有任何** 7790 监听，日志只有 `Status: 用户点击创建大厅：k_ELobbyTypePublic → Host` / `Steam：正在创建大厅……` / `Steam：大厅已创建 :3`；取消勾选后点「启动主机」，`ss` 立刻出现 7790（§1.8 ②）。

模式互斥由 `multiplayer_method_menu` 决定：0 = 直连 LiteNetLib，1 = Steam（`UIMainMenu.cs:1322-1348`）。两个按钮都调 `TransportLiteNetLib.OnWantToConnect(USERINPUT_IPPORT, Net.NetType.Host|Client)`（`:1333`、`:1339-1344`），专用服务器分支传 `DedicatedServer`（`:1327`）。

### 1.3 监听端口的建立点

`TransportLiteNetLib.OnWantToConnect`（`Net.cs:1066`）先 `Net.ShutdownReset()`，再进 `_DoOnWantToConnect`（`:999-1063`）。关键顺序：

    :1022  Net.TransportCreated(type);          // 先置 IsRunning = true、Net.Type = type（:572-573）
    :1023  TransportLiteNetLib transport = new TransportLiteNetLib(ip, port);
    :1024  Net.Transport = transport;           // transport 先挂上
    :1042  bool success = transport.NetManager.Start(port);   // 真正的 bind 在这里
    :1043-1046  if (!success) { DoMultiplayerStatusMessageLog(net_lnl_server_start_failed); return false; }
    :1048  Net.OnTransportConnected();

- `Start(port)` 单参重载 = 绑全部接口。真机：`0.0.0.0:7790`（IPv4）与 `[::]:7790`（IPv6）**同时**出现，属同一 pid（§1.8 ②）。
- 失败时 `OnWantToConnect` 会再调一次 `ShutdownReset()`（`:1073`），而 `ShutdownReset` 把 `Transport` 置 null（`:548-555`）并置 `IsRunning = false`（`:501`）。→ **bind 失败后 `Net.Transport == null`、`IsRunning == false`**，端口自然消失，不会留下半个 transport。
- 客人分支不同：`NetManager.Start()` 无参（`:1030`，绑随机本地端口），然后 `Connect(ip, port, hellopacket)`（`:1036`）。**同样把目标端口存进同一个 `port` 字段**。

### 1.4 失败信号：状态字段是唯一可读的原因

`_DoOnWantToConnect` 的每一条失败路径都写 `Together.Multiplayer.multiplayerStatusMessage` 并返回 false：

| 触发条件 | 代码 | Lang key | EN 文案 | 中文实测 |
|---|---|---|---|---|
| 名字 < 3 字符 | `Net.cs:1001-1004` | `net_name_too_short` | Name too short. | `casualtiestogether_net_name_too_short` = 玩家名称至少需要 3 个字符。 |
| 名字含非法字符（`SanitizeTextInput` 不等） | `:1005-1008` | `net_invalid_name` | Invalid name. | 玩家名称无效。 |
| `TryParseIPPORT` 失败 | `:1009-1012` | `net_invalid_ipport` | Invalid IP:PORT format. | IP:端口格式无效。 |
| 服务器名被内容过滤拒绝（仅 Host/Dedicated） | `:1013-1019` | `net_invalid_server_name` | — | — |
| 客人 `NetManager.Start()` 失败 | `:1030-1034` | `net_lnl_client_start_failed` | LiteNetLib: CLIENT: NetManager.Start() failed. | LiteNetLib：客户端 NetManager.Start() 失败。 |
| 房主 `NetManager.Start(port)` 失败（端口占用等） | `:1042-1046` | `net_lnl_server_start_failed` | LiteNetLib: SERVER: NetManager.Start() failed. | **实测**：LiteNetLib：服务器 NetManager.Start() 失败。 |
| 任意异常 | `:1059-1061` | 无（直接 `DoMultiplayerStatusMessageError`） | `"TransportLiteNetLib.OnWantToConnect:" + ex` | — |
| 不可达兜底 | `:1057` | `net_wanttoconnect_wtf` | — | — |
| 成功 | `:1070` | `net_started`（`{0}` = Host/Client） | Successfully started: {0} | 已成功启动：{0} |

Lang 表位置：`ref/casualties-together/ScavPrototypeMP/assets/locale/EN.json:618-625`、`zh-CN.json:615-622`。

字段本体是 **public static string**：`ScavPrototypeMP/ScavMultiplayer.cs:436` `public static string multiplayerStatusMessage = "Game initialized.";`，写入点 `:613-617`（`log.l("Status: " + msg)`）与 `:619-625`（`log.error("Status ERROR: " + msg)`）。菜单右侧「上一条状态消息」直接拼这个字段：`UIMainMenu.cs:1362`、`ScavMultiplayer.cs:1024-1029`。→ 桥只要反射读一个 public static 字段就能拿到「游戏自己认为发生了什么」。

注意 `DoMultiplayerStatusMessageLog` 是**覆盖写**，没有历史；两次启动之间上一轮的原因已经没了。

### 1.5 QuickTest 的反射入口把原因吞掉

CUCoreLib 侧的两个 QuickTest 入口只是薄封装：

- `ref/CUCoreLib/Networking/MultiplayerBridge.cs:194-202` → `TryStartLocalConnection(address, "Host"|"Client")`
- `MultiplayerBridge.cs:1113-1129`：

      if (!TryResolveRuntime() || _liteNetConnectMethod == null || _netModeType == null) return false;
      var targetAddress = string.IsNullOrWhiteSpace(address) ? "localhost:7790" : address.Trim();
      var result = _liteNetConnectMethod.Invoke(null, new object[] { targetAddress, mode });
      return result is bool connected && connected;

- `_liteNetConnectMethod` 解析自 `OnWantToConnect(string, NetType)`（`MultiplayerBridge.cs:830-831`）。
- **失败分支不写日志**（只有 catch 里写 `"CUCoreLib failed to start KrokMP localhost quick test mode."` + ex，即异常才有），返回值只有 bool。→ `mp.host` 现在只能看到「returned false」，无法区分「名字太短」「ipport 非法」「端口占用」「Together 未装」。
- 缺省地址 `localhost:7790`（`ref/CUCoreLib/Helpers/LaunchOverrideManager.cs:21` `DefaultQuickTestAddress`）。
- 身份写入同一份静态字段：`TryConfigureLocalIdentity` 写 `UIMainMenu.USERINPUT_NAME` / `USERINPUT_IPPORT`（`MultiplayerBridge.cs:176-192`，`SetStaticString` `:1131-1148`）。所以 `mp.host` 的 `address` 参数确实会成为 `TransportLiteNetLib.port` 的来源。
- `MultiplayerBridge.Initialize` 只有在 `TryResolveRuntime()` 成功时才 `IsAvailable = true`（`:228-233`），否则 `ScheduleRetry()`。

### 1.6 桥侧现状（CuprumBridge 独立 assembly）

- 已存在的通用反射解析器：`mods/cuprum-bridge/CuprumBridge.Plugin/MpBackend.cs:83-102` `FindType`，先遍历 `AppDomain.CurrentDomain.GetAssemblies()` 调 `assembly.GetType(fullName, false)`，再对 `AssemblyNames`（当前只有 `CUCoreLib`、`net.cucorelib`，`:14`）逐个 `Assembly.Load(name).GetType(...)`，逐个 try/catch。找不到返回 null，不抛。
- `Type ApiType() => _apiType ?? (_apiType = FindType(ApiTypeName));`（`:79`、`:81`）：`_apiType` 为 null 时每次都会重试查找，所以晚加载的程序集也能被后续命令看到（缓存只缓存成功结果）。
- `MpBackend.State()`（`:22-43`）组装的 JSON 字段：`available`、`running`、`client`、`server`、`host`（`:15` 的 `StateProperties` 去掉 `Is` 前缀并小写）、`bridge_receivers_installed`、`krokmp_version`。**没有 port**。
- `MpBackend.Start()`（`:45-77`）把 QuickTest 的所有失败都映射成 `Unavailable` + 固定文案（`:64-65` `"MultiplayerBridge." + startName + " returned false"`）。
- 错误码集合：`CuprumBridge.Core/Contract.cs:48-56` —— `unknown_command` / `denied` / `invalid_params` / `busy` / `unavailable` / `timeout`。
- 插件 GUID `dev.cuprum.bridge`（`BridgePlugin.cs:9`）；工程 `netstandard2.1`，只引用 BepInEx / UnityEngine / Newtonsoft（`CuprumBridge.Plugin.csproj`），**不引用 CasualtiesMP 或 CUCoreLib**，因此 Together 只能反射。
- 桥已把游戏 pid 报给宿主：`BridgeSession.cs:279` `["pid"] = CurrentPid()`（`:287-297`）。

### 1.7 Cuprum 侧现状（端口在两个地方各自为政）

- `crates/core/src/bridge/mp.rs:6-15`：`MpState { available, running, client, server, host, krokmp_version }`，serde 默认字段名与 JSON 一致，**无 port**。未知字段被忽略（`from_value` 无 `deny_unknown_fields`，`:18-20`），所以 C# 侧加字段是向后兼容的。
- 轮询：`crates/app/src/ui/bridge.rs:11` `POLL_MILLIS = 2_000`，循环里锁 Hub 调 `bridge_snapshot()`（`:20-52`）；`crates/core/src/hub/bridge_host.rs:96-134` 里只有 `status.running && status.session.is_some()` 才 `poll_mp_state()`（`:116-120`），后者是 `server.invoke_silent("mp.state", json!({}))`（`:130-134`）。请求超时 2 秒（`crates/core/src/bridge/server.rs:37` `request_timeout: Duration::from_secs(2)`）。
- **另一套端口**：Multiplayer 页有自己的端口输入，默认 `"25565"`（`crates/app/src/ui/multiplayer.rs:91`），它是 EasyTier 的转发/白名单端口，经 `hub.start_host(port)`（`crates/core/src/hub.rs:757`）/ `join_room(input, port)`（`:778`）→ `TunnelConfig { port }`（`crates/core/src/easytier/tunnel.rs:30-37`）进入隧道。游戏里真正监听的端口（默认 7790）Cuprum **完全不知道**。
- 端口在进程参数里烧死：host 侧 `Argument::TcpWhitelist(port)` / `UdpWhitelist(port)`（`tunnel.rs:76-77`），guest 侧 `Argument::PortForward`（`:88-96`）；主机名也由端口派生 `host_hostname(port) = HOST_PREFIX + port`（`crates/core/src/room.rs:151-153`），guest 靠这个名字发现房主（`room.rs:155-172`、`tunnel.rs:161-188`）。改端口 = 必须重启 EasyTier 进程；`Tunnel` 没有 set_port，唯二的重启是 guest 首次发现房主（`tunnel.rs:172-185`）和 Drop（`:211`）。

### 1.8 真机实测记录

环境：`steam -applaunch 4576490`，菜单底部「打开多人联机模组菜单」→ 右侧「使用 Steam」复选框。

命令：

    ss -lunp | grep ':7790'
    grep -a 'Status' "<game>/BepInEx/LogOutput.log" | tail -8

| # | 操作 | 观测 |
|---|---|---|
| ① | 勾选「使用 Steam」，点「创建大厅」 | `ss` 无 7790；日志 `Status: 用户点击创建大厅：k_ELobbyTypePublic → Host` / `Steam：正在创建大厅……` / `Steam：大厅已创建 :3` |
| ② | 取消勾选 → 面板变「通过 IP 直接连接」，字段 `127.0.0.1:7790`，点「启动主机」 | 日志 `Status: 已点击「创建主机」！` + `Status: 已成功启动：Host`；`ss` 出现 `UNCONN 0 0 0.0.0.0:7790`（fd=470）与 `UNCONN 0 0 [::]:7790`（fd=606），同 pid 565551 |
| ③ | 停止服务器 → 输入改 `127.0.0.1:24444` → 启动主机 | 7790 消失，出现 `0.0.0.0:24444` 与 `[::]:24444`；日志同样两行 `已点击「创建主机」！` / `已成功启动：Host` |
| ④ | 先 `python3` 绑 `0.0.0.0:25000`，输入改 `127.0.0.1:25000` 再启动主机 | 日志 `at LiteNetLib.LiteNetManager.BindSocket (...) , errorCode: AddressAlreadyInUse`，随后 `Status: LiteNetLib：服务器 NetManager.Start() 失败。`；UI「上一条状态消息」同文；游戏未占住 25000 |
| ⑤ | QuickTest 单机自连 | **未验证**。需要 `multiplayerQuickTest` 开关（`CUCoreLib.cfg`）与两实例角色认领（`LaunchOverrideManager.cs:109-157`），属「改配置才能复现」的失败注入，没有做 |

插件加载顺序（本次日志）：`Together 5.5.4`（第 16 行）→ `CuprumBridge 0.1.0`（43 行）→ `多模组汉化` → `CUCoreLib 1.1`（47 行）→ `QoL Unknown` → `Bark`。顺序由 BepInEx 排序决定，**不能当成保证**（见 §2.2）。

---

## 2. 端口检测机制

### 2.1 结论

**机制：CuprumBridge 在组装 `mp.state` 时，用反射从 `AppDomain` 里拿 `Together.Net.Transport`，再读实例上的 `port` 字段；读不到就回 `null`，永不抛异常。**

成立的三条依据：

1. **同进程**。BepInEx 5 把所有插件装进同一个 AppDomain（真机日志里 `Together` 与 `CuprumBridge` 并列在 `6 plugins to load` 下），两者共享静态类 `Together.Net`。桥已经用同一手法读 `CUCoreLib.Networking.MultiplayerApi`（`MpBackend.cs:83-102`），不是新机制。
2. **成员是公开的**。`Net.Transport` 的 getter 是 public（`Net.cs:221` `{ get; internal set; }`，`internal` 只限制 set），`TransportLiteNetLib.port` 是 `public readonly ushort`（`:951`）。readonly 只挡写，`FieldInfo.GetValue` 照读。
3. **不依赖具体 transport 类型**。取 `transport.GetType()` 的具体类型再找 `port` 字段：LiteNetLib 有、Steamworks 没有（§1.2），所以 Steam 大厅天然回 `null`，不需要先判断角色。

原方案（`pid` + `/proc` 或 `ss`）见 §2.8，已废弃。

### 2.2 最小实现（CuprumBridge 侧，示意）

沿用现有 `FindType` 的形状；**不要**在插件加载时解析并缓存（Together 可能后于 CuprumBridge 加载，真机顺序只是「这次恰好如此」）。

    // 常量补上 "CasualtiesMP"，其余不动
    static readonly string[] TogetherAssemblies = { "CasualtiesMP" };

    static ushort? ListeningPort()
    {
        try
        {
            var net = FindType("Together.Net");                 // 与 CUCoreLib 同一套 FindType
            if (net == null) return null;                       // Together 未装 / 未加载
            var transport = net
                .GetProperty("Transport", BindingFlags.Public | BindingFlags.Static)?
                .GetValue(null, null);
            if (transport == null) return null;                 // 未开房，或启动失败后 ShutdownReset 置 null
            var isClient = net.GetProperty("IsClient", BindingFlags.Public | BindingFlags.Static)?
                .GetValue(null, null) is bool client && client;
            if (isClient) return null;                          // 客人侧 port 是目标端口，不是监听端口
            var field = transport.GetType()
                .GetField("port", BindingFlags.Public | BindingFlags.Instance);
            if (field == null || field.FieldType != typeof(ushort)) return null;   // Steam transport
            return (ushort)field.GetValue(transport);
        }
        catch { return null; }                                  // 任何反射异常都不外泄
    }

要点：

- `FindType("Together.Net")` 用 `assembly.GetType(fullName, false)`（`MpBackend.cs:87`）——类型不存在时返回 null 而不抛；`Assembly.Load("CasualtiesMP")` 走 try/catch（`:90-99`），BepInEx 下通常已加载，兜底失败也无害。
- 读 `Transport` 的属性 getter 可能触发 `TypeInitializationException`（`Together.Net` 静态构造失败），被外层 catch 吃掉。
- **不要缓存 null**：现有 `ApiType()` 的 `??` 写法每轮都会重试，正好符合「Together 后加载」的情况；若改成只在 Init 时算一次，就必须在结果为 null 时重试。
- 编辑器 / 无 Together：`FindType` 返回 null → `port: null`，`available` 仍按 CUCoreLib 报，桥不降级、不抛异常。

### 2.3 失败返回值（约定：永不抛，字段可缺省）

| 情形 | `ListeningPort()` | `mp.state` 表现 |
|---|---|---|
| CUCoreLib 未加载 | — | 维持现状：整个 `mp.state` 返回 `unavailable`（`MpBackend.cs:24-26`） |
| Together 未装 / `Together.Net` 找不到 | `null` | `port: null`，`available` 由 CUCoreLib 决定 |
| 未开房（`Transport == null`） | `null` | `port: null`，`running: false` |
| 启动失败（bind 失败后 ShutdownReset） | `null` | `port: null`（不会残留旧端口，§1.3） |
| Steam 大厅主机 / Steam 客户端 | `null` | `port: null`（无本地端口，UI 不该显示端口） |
| 直连客人 | `null` | `port: null`（`port` 字段是目标端口，见 §2.4） |
| 直连房主 / 专用服务器 | 实际监听端口 | `port: 7790`（或用户在游戏里改成的值） |

### 2.4 客人口径：`port` 只对 server 回

`TransportLiteNetLib.port` 在两种角色下含义不同（`Net.cs:956-958`、`:1030-1036`）：

- 房主：`Start(port)` → `port` 是**监听端口**。
- 客人：`NetManager.Start()` + `Connect(ip, port, ...)` → `port` 是**房主端口（目标端口）**。

因此判断条件必须是 `Net.IsServer`（`Net.cs:214` `IsServer => !IsClient`，含 DedicatedServer），而不是「有 transport 就回」。否则 UI 会把客人的目标端口当成本机监听端口显示，用户会拿它去填隧道端口，隧道就指错了方向。

桥侧对照：`mp.state` 已经分别回 `client` / `server` / `host` 三个 bool（`MpBackend.cs:15`、`Contract.cs:60-62`），Rust 侧 `MpState` 同名（`crates/core/src/bridge/mp.rs:9-12`），判据现成。

### 2.5 线格式与 Rust 侧改动建议（供后续实现任务引用）

C# 侧：`state["port"] = ListeningPort();`（`ushort?` → JSON number 或 null）。因为 `MpState::from_value` 用 serde 默认（无 `deny_unknown_fields`），加字段不会打断老版本 Cuprum：

- Rust：把 `#[serde(default)] pub port: Option<u16>` 加到 `crates/core/src/bridge/mp.rs:7-15`，旧桥回包照常解析（缺字段 → `None`）。
- 语义写死：**`port` 只在 `server == true` 时非空**；`client` 恒为 `null`。`mp.state` 的 `port` 是「本机游戏监听端口」，不是「隧道端口」（隧道端口仍由 `MultiplayerUi.port` 管，`crates/app/src/ui/multiplayer.rs:27`、`:91`）。
- 如果将来 `mp.host` 的返回也带端口，必须同样只在 host 分支给值。

### 2.6 `mp.host` 的 error 映射建议

桥当前把 QuickTest 的 false 一律变 `unavailable` + `"...returned false"`（`MpBackend.cs:64-65`），信息量为零。建议：**调用前后各读一次 `Together.Multiplayer.multiplayerStatusMessage`，变了就用新的**，再按下表映射：

| 游戏侧事实（状态字段 / 类型探测） | 建议 code | 建议 message | 依据 |
|---|---|---|---|
| `FindType("Together.Net") == null` | `unavailable` | `CasualtiesMP (Together) is not loaded` | §2.3 |
| `MultiplayerApi` 缺失 | `unavailable` | 现状文案保留 | `MpBackend.cs:24-26` |
| `net_name_too_short`（< 3 字符） | `invalid_params` | `KrokMP: name too short (min 3 characters)` | `Net.cs:1001-1004`；`EN.json:618` |
| `net_invalid_name` | `invalid_params` | `KrokMP: invalid name` | `Net.cs:1005-1008` |
| `net_invalid_ipport` | `invalid_params` | `KrokMP: invalid IP:PORT format` | `Net.cs:1009-1012` |
| `net_invalid_server_name` | `invalid_params` | `KrokMP: server name rejected` | `Net.cs:1013-1019` |
| `net_lnl_server_start_failed` | `busy` | `KrokMP: NetManager.Start() failed (port busy?)` | `Net.cs:1042-1046`；真机 ④ `AddressAlreadyInUse` |
| `net_lnl_client_start_failed` | `unavailable` | `KrokMP: client NetManager.Start() failed` | `Net.cs:1030-1034` |
| 状态字段含 `OnWantToConnect:` + 异常 | `unavailable` | 截到 200 字以内的异常首行 | `Net.cs:1059-1061` |
| 其它 false | `unavailable` | 带上状态字段原文 | 兜底 |

理由：`busy` 在这套协议里表示「现在做不了、换个参数/稍后可以」（`Contract.cs:53`），端口占用正是这一类；`invalid_params` 对应「用户给的输入不合法」，UI 已按 code 分区处理（`crates/app/src/ui/mp_control.rs:93-107` 的 `game_error_params` 与 `stops_auto`）。注意：状态字段会被语言包本地化（中文实测就是中文），**不要按字符串内容做业务判断**——它只用来填错误文案，成败仍以 QuickTest 返回 false 为准。

### 2.7 端口变化 → hub 更新 → 白名单重建：可见窗口与风险

链路（源码与实测可对）：

1. 用户在游戏里「停止服务器」→ 改端口 → 「启动主机」：旧 transport `Shutdown`、`Transport = null`（`Net.cs:548-555`），新 transport 绑新端口（`:1023-1042`）。
2. Cuprum 的 2 秒轮询（`crates/app/src/ui/bridge.rs:11`）下一拍拿快照，且必须满足 `status.running && status.session.is_some()`（`bridge_host.rs:116-120`），再等 `mp.state` 一次往返（超时上限 2 秒，`crates/core/src/bridge/server.rs:37`）。
3. 所以**从端口真正变化到 UI 拿到新 port，最坏约 2 s（轮询间隔）+ 2 s（请求超时）≈ 4 s**；典型 2 s 出头。中间可能采到「停服中」的一拍（`running=false, port=null`），UI 必须容忍 port 有→无→有。
4. **第 3 步之后没有任何自动动作**：`Tunnel.port` 在 `Tunnel::start` 时定死（`tunnel.rs:126-159`）；host 的 `--tcp/udp-whitelist`（`:76-77`）、guest 的 `--port-forward`（`:88-96`）、host 主机名 `host_hostname(port)`（`room.rs:151-153`）全是启动参数。

风险按严重度：

- **高：主机名不匹配 → 客人永远发现不了房主**。guest 用自己配置的 port 派生 hostname 去 peer 列表里找（`room.rs:155-172`），host 用真实端口派生。两者不同时 guest 连 `find_host` 都过不了，表现是「隧道通了但一直等房主」，没有任何报错指向端口。
- **高：白名单/转发指向旧端口**。host 侧 EasyTier 只放行旧端口；guest 侧 `--port-forward 0.0.0.0:<旧> → <host>:<旧>`。游戏即使在新端口监听，流量也过不去（UDP 包被丢，最像「能连上但进不去」）。
- **中：那 ≤4 s 里用户可能点「复制地址」**，拿到旧端口（`crates/app/src/ui/mp_control.rs:76-84`、`pages/multiplayer.rs:716`）。当前 `game_address` 用的是 UI 端口而不是游戏端口，这个错位本来就存在。
- **中：采到瞬态 `port: null`** 时不能把它当「没有端口」去做破坏性重排（例如清空白名单再重建），否则停服/重开一次就把隧道拆了。
- **低：`status.session` 断开期间（桥重连）不刷新**，`bridge_snapshot` 跳过 `poll_mp_state`（`bridge_host.rs:116-120`），UI 停在旧值，最长到桥重连退避（1/2/5/10/30 秒）。

结论：**端口检测本身不改变隧道**。要让端口变化真正生效，必须显式重启 EasyTier（host 重建 whitelist 与 hostname，guest 重建 forward），且要等 `mp.state` 连续采样一致后再做，不能跟着 2 秒轮询抖。检测只是把「现在真实端口是多少」变成可观测事实。

### 2.8 废弃：`/proc/<pid>/net` 或 `ss` + pid 方案

原方案是「桥把游戏 pid 报给宿主（已经报了，`BridgeSession.cs:279`），宿主用 `/proc/<pid>/net/udp` 或 `ss -lunp` 找该 pid 的监听端口」。废弃理由：

1. **同进程能直接读，没必要出进程再解析**。桥就住在游戏进程里，`transport.port` 是内存里的一个 `ushort`；走 `/proc` 等于把已知答案编码成文本再解码回来，多一层失败面（fd 泄漏、`wineserver` 影子 fd、权限、宿主与游戏不同 mount namespace）。
2. **实测就撞上了**：`ss -lunp` 里 `0.0.0.0:7790` 同时挂 `("CasualtiesUnkno",pid=565551,fd=470)` 与 `("wineserver",pid=565405,fd=2456)`，`[::]:7790` 又是另外两个 fd；`/proc/net/udp` 按 inode 反查还得处理这种重复归属，同一份信息要额外决定「选哪个 fd」「是否同一端口」。
3. **Windows 同样成立**。真要支持 Windows 原生路径（非 Wine），`/proc` 不存在，得换 `GetExtendedUdpTable` 或 `netstat -ano`，又是另一套实现与失败模式；反射读字段跨平台完全一样。
4. **时序更准**。读字段拿到的是「transport 认为的端口」，与 `NetManager.Start(port)` 的参数同源；`ss` 反映「内核里现在有什么」，在 bind 失败/重置的瞬间会给出与游戏状态不一致的答案（真机 ④：状态字段说失败，端口其实从未被占）。
5. **口径问题**。`/proc` 会把客人侧随机本地端口也列出来，需要额外规则才能只挑「服务端口」；而 `IsServer` 是游戏自己的语义（`Net.cs:214`）。

pid 只保留「桥会话识别」用途（`status.session.pid`，用于重启后重置自动状态机，见 `docs/bridge/w4-multiplayer.md:66`），不再用于端口发现。

---

## 3. 真机回归工单（索引）

本波工单已按体量拆到 [docs/bridge/w4-tickets/](w4-tickets/README.md)。本文件只保留 §1 技术事实与 §2 端口机制，§3 只做索引。

- 总览（使用约定、覆盖矩阵、记录模板）：[README.md](w4-tickets/README.md)
- 环境与硬前置、取证命令、注入与恢复：[commands.md](w4-tickets/commands.md)
- 偏差原文与队长裁决：[decisions.md](w4-tickets/decisions.md)

用例：
- [R1](w4-tickets/R1.md) 房主先开房、客人后进
- [R2](w4-tickets/R2.md) 客人先进隧道、房主后开房
- [R3](w4-tickets/R3.md) 桥卡片开关（默认关）
- [R4](w4-tickets/R4.md) QuickTest 失败注入（4 组，各一次）
- [R5](w4-tickets/R5.md) 游戏退出
- [R6](w4-tickets/R6.md) 桥断开
- [R7](w4-tickets/R7.md) 端口自动检测
- [R8](w4-tickets/R8.md) 游戏内直接开房
- [R9](w4-tickets/R9.md) 换房 / 重进
- [R10](w4-tickets/R10.md) 房主退出后客人侧
- [R11](w4-tickets/R11.md) 房主换端口后的客人侧

裁决摘要：D1/D2/D3/D4/D6/D7 接受；D5 为改进项（不阻塞本轮）；D8 为产品缺口（不阻塞本轮）。执行口径见 [decisions.md](w4-tickets/decisions.md)。


## 附：一句话摘要

端口是 `Together.Net.Transport`（`Net.cs:221`）指向的 `TransportLiteNetLib.port`（`:951`），只在 server 角色下等于监听端口，由 `NetManager.Start(port)`（`:1042`）绑定全部接口；读它是同进程反射的一小段代码，失败原因由 `Together.Multiplayer.multiplayerStatusMessage`（`ScavMultiplayer.cs:436`）给出；Cuprum 侧 2 秒轮询只能「看到」，不会自动重建隧道，端口变化必须显式重启 EasyTier。

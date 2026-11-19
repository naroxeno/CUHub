# CU Bridge 实施方案

这一段解决的是「EasyTier 把双方放进同一虚拟网段之后」的事：谁在游戏里开房间、客人连哪个 IP、状态怎么回到 Cuhub。线协议见 [protocol.md](protocol.md)，C# 侧 API 研究见 [csharp-plugin.md](csharp-plugin.md)，Rust 侧现状见 [core/bridge/overview.md](../core/bridge/overview.md)。

> 变更（2026-11-20）：下文关于 rendezvous 发现文件与临时端口的方案已被固定端口 `127.0.0.1:41297` + 两端共享 token 取代，以实现与 [protocol.md](protocol.md) 为准；这里保留原始决策记录。

## 0. 结论

- EasyTier 只保证房主在 `10.144.144.1` 可达；开房/入房由游戏本体负责，Cuhub 不代劳。
- 控制游戏分两条路：运行时桥 → 手动填 IP。游戏经 Steam `steam://rungameid` 启动，没有传参通道，启动参数路线不存在。
- 桥不重写反射。CUCoreLib 的 `MultiplayerApi` 是游戏侧唯一稳定入口；CT 自身没有给第三方 mod 的网络 API。
- 本机 IPC 继续 TCP loopback + rendezvous 文件。命名管道在 Proton 下原生 Rust 进程连不进去。

## 1. 两条控制路径

| 路径 | 前置 | 适用 | 代价 |
|---|---|---|---|
| A 运行时桥 | 插件已装、游戏在运行 | 游戏已开、玩家忘了开房 | 依赖 CUCoreLib 与反射；纯原版模式不可用 |
| B 手动 | 隧道已连 | 桥没装或桥失败 | 玩家自己在 CT 菜单点 Host/Join 并填 IP |

A 的形态：`mp.host` / `mp.join` 到达插件后，调 CUCoreLib `TryConfigureLocalIdentity(name, address)` + `TryStartLocalQuickTestHost/Client(address)`，全程主线程。

B 的形态：Cuhub 把 `10.144.144.1:7790` 显示出来，供玩家复制。

## 2. C# 侧布局

```
<game>/BepInEx/plugins/CuhubBridge/
  CuhubBridge.Plugin.dll      # 唯一 BepInPlugin 入口：探针与加载器
  CuhubBridge.Core.dll        # 协议常量、帧、rendezvous、TCP 客户端、命令契约
```

- 整体是一个目录型模组，Cuhub 的 mod 列表里只出现一条；启停、删除、更新按目录走。MVP 先给两个 DLL；MpBackend / Transport 的拆分等命令面变大再做。
- 只有 Plugin 带 `[BepInPlugin]`，其余 DLL 由 Plugin 在启动时加载。BepInEx 会递归扫描并尝试把每个 DLL 当插件，找不到 `BaseUnityPlugin` 自动跳过，无害；但别给它们也标 `[BepInPlugin]`。
- 主 DLL 只做：读 rendezvous、连宿主、心跳、按方法名转给 Transport/Mp、把结果回填。业务不在这里。
- CUCoreLib 是软依赖 + 全反射，不项目引用 `CUCoreLib.dll`（缺库时直接引用会 `TypeLoadException` 拖垮整个插件）。反射结果缓存，每命令几次调用，开销可忽略。

线程模型：

- 读线程只解帧和入队，不碰 Unity、不碰 CUCoreLib。
- 所有游戏侧调用在 `Update()` 的主线程泵里执行；结果进写队列，写线程串行发出。
- `CUCoreUtils.CallWhen` / `StartCoroutine` 也必须从主线程调。

## 3. 命令落点

沿用 [protocol.md](protocol.md#L110-L127) 的命令表，不另立 id。

| 命令 | 落点 | 备注 |
|---|---|---|
| `mp.state` | 反射 CUCoreLib `MultiplayerApi` 的 IsAvailable / IsRunning / IsClient / IsServer / IsHost 与版本 | 玩家名与人数待真机 |
| `mp.host` / `mp.join` | `TryConfigureLocalIdentity` + `TryStartLocalQuickTestHost/Client` | 语义待验证 |
| `mp.leave` | 回主菜单入口待确认 | 找不到就回 `unavailable` |
| `mp.kick` | CT 控制台 `kick` | 程序化执行入口待确认 |
| `save.*` | `GetKrokMpSaveScope`（internal，反射） | 只读；写档仍由用户确认 |
| `skin.*` | 本地文件 | 与联机无关 |

状态推送不用 Harmony patch 联机事件，1 秒轮询 + diff 更稳：patch 别人的 patch 容易打架。

## 4. Rust 侧改动

| 位置 | 改动 |
|---|---|
| `bridge.rs` | 补服务器：监听、会话、握手、命令表、权限、审计。协议已定，照抄 |
| `Hub` | 新增 `start_bridge(program_data)` / `stop_bridge()` / `bridge_status()` / `invoke_bridge(cmd, params)` / `take_events()`；`game_session()` 留给 M2 UI |
| `launch.rs` | 不改：Steam `steam://rungameid` 启动没有参数通道，控制全走桥 |
| `multiplayer.rs` | M2 再做：新增游戏侧状态字段，独立于 `TunnelState`（隧道通 ≠ 游戏已开局） |
| `rendezvous.rs` | `start_bridge` 写、`stop_bridge` 删除；拉起游戏前确认未过期留待 UI 接线 |
| 设置 | 权限存 `Settings.json` 的 `bridge.permissions` |

`bridge.json` 在 prefix 下（Proton 的 `drive_c/ProgramData/CasualtiesHub/`），不进 appdata；`<appdata>/bridge.lock` 的 flock 保证单宿主。

## 5. 部署与生命周期

桥是内置目录型模组：扫描、启停、删除仍交给 `mods` 模块，安装走 `install` 的 staging → plan → 备份 → apply 链路，不另做链接与版本层：

- 安装：Cuhub 把内置的 `CuhubBridge/` 目录放进 `Plugins/CuhubBridge`，入口是 Local Mods 卡片底部的独立安装区块（优先用应用旁的 `CuhubBridge.zip`，没有则选本地 zip）；只接受 `CuhubBridge.Plugin.dll` 与 `CuhubBridge.Core.dll` 并校验 GUID，目标被 Links 托管时拒绝。
- 识别：普通目录型模组，mods 列表里显示一条；启用、禁用、删除走现有 `set_mod_enabled` / `delete_mod`。
- 纯原版：在 mods 里禁用该条目（或整表禁用），联机功能随之不可用，界面说明原因。
- 更新：不做版本比较与差异，随 Cuhub 安装包整体替换。
- 缺 CUCoreLib 时插件仍加载，`mp.*` 回 `unavailable` 并在状态里报「缺 CUCoreLib」。CUCoreLib 与 CT 是第三方模组，Cuhub 不代装，只提示去 Nexus。
- 卸载：删目录本身，限制仍是 Plugins 一级条目。

## 6. 玩家看到的流程

W4 的具体实现方案（权限开关、自动状态机、代码落点）见 [w4-multiplayer.md](w4-multiplayer.md)；代码与单测已落地，待真机回归。

房主：联机页建房 → 隧道 `Connected` → 「启动游戏」（Steam 拉起，不带参）→ 桥连上 → 页面「在游戏内开房」→ 桥调 `mp.host`。  
客人：输码 → 等隧道 `Connected`（此时才知道 `10.144.144.1`）→ 启动游戏 → 桥连上 → 自动 `mp.join`。  
桥不在（没装、纯原版、握手失败）时走 B 路径：页面显示 `10.144.144.1:7790` 与复制按钮，玩家自己在 CT 菜单点 Host/Join。  
状态两行分开写：「隧道：已连接（10.144.144.1）」与「游戏内：房主 / 客户端 · N 人」。中继要显式标出，不能显示成直连。

回退：任一环失败，页面保留 `10.144.144.1:7790` 与复制按钮，并提示「在 CT 菜单里手动填写」。

## 7. 分期

1. **M1 隧道 + 手动路径**：隧道状态、两行 UI、`10.144.144.1:7790` 展示与复制。玩家在 CT 菜单手动开房/加入，先跑通一次真实联机；`launch_game` 不动。
2. **M2 只读桥**：Rust 侧监听与握手、探针 DLL、`mp.state`、`game.state` 推送，页面显示游戏内状态。宿主侧与 `mp.state` 已在 MVP 落地；页面与 `game.state` 未做。
3. **M3 控制**：`mp.host` / `mp.join`（桥卡片开关 + 房主自动一次 / 客人自动最多 3 次，手动按钮与重试兜底）、权限活更新；`mp.kick` 与两段式确认留到后续，`mp.leave` 因 C# 未实现暂不接。
4. **M4 收尾**：`save.*` / `skin.*`、审计、失败提示文案。

## 8. 风险与真机验证

- `TryStartLocalQuickTestHost/Client` 名字带 QuickTest，可能只是测试入口，不等价于 UI 的 Host/Join。
- `mp.leave` 与 `mp.kick` 没有公开 API，可能只能靠控制台；Harmony 注入属高风险，单独立项。
- CT 更新会改 `Together.*` 成员，反射与 CUCoreLib 一起碎。握手里上报 `KrokMpVersion`，宿主给「桥版本不匹配」的明确提示。
- Proton 下 loopback 通断、双实例抢锁、游戏崩溃后 rendezvous 残留，都进 `bridge-selftest`。
- 游戏只能经 Steam `steam://rungameid` 拉起、无法带参：没有桥时只能手动，这个限制要在 UI 写清。

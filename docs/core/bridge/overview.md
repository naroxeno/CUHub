# bridge（游戏内联动）

宿主侧的桥：监听固定回环端口、维护与单个游戏进程的连接、执行命令白名单。不负责游戏内的事，也不替 mod 干活。宿主不在 app 启动时起，由 app 按游戏进程状态启停。

## 实现状态（2026-11-20，实验性，不发布）

Rust 宿主已落地：`proto.rs`（帧编解码）、`command.rs`（命令表与权限）、`session.rs`（连接读写、心跳、请求等待）、`handshake.rs`（hello 校验、共享 token）、`server.rs`（固定端口监听、单会话、最近握手失败、事件、审计）、`mp.rs`（`mp.state` 回包解析），以及 `hub/bridge_host.rs` 上的生命周期、状态快照与联机控制开关。`crates/core/tests/bridge_server.rs` 用回环 TCP 假客户端跑 14 个集成测试；`cargo test -p cuhub-core` 全绿。

app 已接线（W2/W3）：`Ui::watch_bridge` 每 2 秒取一次 `Hub::bridge_snapshot`；检测到游戏进程才启动桥、退出后停止；状态栏显示未运行 / 未连接 / 已连接（pid）与最近一次握手失败；Local Mods 按 hello 的 guid 快照标“运行中已加载”；Multiplayer 页顶部卡片显示 `mp.state`。W4 接线：桥卡片可开关 `mp.host` / `mp.join` 授权；房间卡显示游戏内房间状态与地址，手动命令走两段式确认；`ui::mp_control` 负责自动状态机（房主一次、客人 3 次退避、`denied` / `invalid_params` 即停）。

游戏侧同时有可编译的 MVP：`CuhubBridge.Core`（协议常量、消息、帧、固定端点与共享 token、TCP 客户端）与 `CuhubBridge.Plugin`（探针、握手、心跳、`mp.*` 反射）。宿主按 C# MVP 的实际报文对齐：hello 是普通 `req`，失败用 `res.ok=false` 回 `bad_token` / `proto_mismatch` / `duplicate_pid` / `busy`，`cmd.invoke` 的 `nonce` 与 `id` 都按字符串收发。`dotnet build -c Release` 通过且无警告，尚未真机加载。

联机状态经 CUCoreLib 的 `MultiplayerApi` 读，不直连 Casualties: Together。插件对 CUCoreLib 是运行时软依赖：全反射调用 `MultiplayerApi` / `MultiplayerBridge`，缺库时 `mp.*` 回 `unavailable`，插件本身照常加载。注意 CUCoreLib 文档里的 "bridge" 指它自己的 KrokMP 集成，与本模块同名不同物。

## 模块

| 文件 | 职责 |
|---|---|
| `bridge/proto.rs` | 长度前缀帧、JSON `Message`、`ErrorBody`、`MAX_FRAME` |
| `bridge/command.rs` | 15 条命令表、`Policy`、`Permission`、按 guid 解析权限、hello 快照 |
| `bridge/session.rs` | 帧读取、`cmd.invoke` 发送与超时、pending 表、心跳、上行事件过滤 |
| `bridge/handshake.rs` | 首帧 hello、共享 token 常量、proto / mods 校验、hello.ok / 拒绝 |
| `bridge/mp.rs` | `mp.state` 回包的 `MpState` 解析 |
| `bridge/server.rs` | 监听 `127.0.0.1:41297`、单会话、最近握手失败、事件、审计 |
| `bridge/install.rs` | 固定目标 `Plugins/CuhubBridge`、`PLUGIN_FILE` / `CORE_FILE`、payload 校验 |
| `hub/bridge_host.rs` | 启停状态机、`bridge_snapshot`、`invoke_bridge`、`take_events`、桥安装与只读状态 |

## 职责

- 固定监听 `127.0.0.1:41297`；token 是编译进两端的共享常量，不再生成、不落盘。
- 握手、心跳、帧编解码、命令表与权限判定。
- 由 app 的 `bridge_snapshot` 按游戏进程状态启停；端口被占用时只报状态，不 panic。
- 把游戏侧上行事件翻成 `HubEvent::Bridge`，其余一概经 `Hub`。
- 把宿主侧的 `cmd.invoke` 编码、发送、超时、审计；`mp.state` 走静默调用，不刷日志。

## 边界

- 不做 UI：状态只进快照与事件；app 轮询快照写 signal，渲染路径不取 Hub 锁。
- 不碰凭据：`nexus_auth` 的内容不进桥。
- 不做文件读写通道：命令 handler 在游戏侧，宿主不替它读写磁盘。
- 不做 prefix 推导：没有 ProgramData 参数，也不遍历文件系统；端口固定，找不到宿主就是连不上。
- 只做协议、白名单与权限判定；命令语义全在游戏侧。

## 命令表

| policy | 命令 |
|---|---|
| read（默认允许） | `mp.state`、`save.list`、`save.current`、`skin.current`、`skin.list` |
| control（默认拒绝） | `mp.host`、`mp.join`、`mp.leave`、`skin.apply`、`skin.reload` |
| mutate（默认拒绝） | `mp.kick`、`save.save`、`save.load`、`save.backup`、`save.restore` |

命令表只增不删；宿主对未注册命令回 `unknown_command`，无权限回 `denied`，都发生在发出 `cmd.invoke` 之前。

## 权限

- 持久化在 `Settings.json` 的 `bridge.permissions`：mod guid → 命令 id → `granted` / `denied` / `ask`。
- 只考虑本次 hello 声明的 mod guid；同名命令在多个 guid 下解析时 `denied` 优先，其次 `ask`，再次 `granted`。
- 没有任何条目时按 policy 默认：read 允许，control / mutate 拒绝。
- `ask` 在 MVP 里等同拒绝（还没有确认 UI），但 hello.ok 的 `perms` 仍如实报 `ask`。
- 运行中改权限经 `BridgeServer::set_permissions` 立即生效：`ServerShared.permissions` 是 `RwLock`，每条命令发出前现取现判；hello.ok 的 `perms` 只在握手时取一次，之后不重发。
- 授权变更不在握手里做；协议要求的 `perms.changed` 尚未实现。

## Hub 接口与状态

- `start_bridge() -> Result<BridgeStatus, HubError>`：无参数，重复调用返回现有状态；端口被占为 `BridgeError::PortInUse`。
- `stop_bridge()`：清会话、停监听；`bridge_status()` 返回 `running`、`endpoint`、`session`、`last_error`。
- `bridge_snapshot(&mut self) -> BridgeSnapshot`：读 `game_running`，按需启停桥，会话存在时调用 `mp.state`，返回 `{ game_running, status, mp, control, start_error }`；同时排空事件通道，避免无消费者时无限增长。
- `bridge_control_enabled() -> bool`：`bridge.permissions` 里 `dev.cuhub.bridge` 的 `mp.host` 与 `mp.join` 都为 `granted` 才算打开；只认这两条，其它 guid 与命令不参与判断。
- `set_bridge_control(bool) -> Result<(), HubError>`：只改上述两条并写回 `Settings.json`；桥在运行时立刻 `BridgeServer::set_permissions`，已连会话的下一条命令即按新权限判定；写一条 `Bridge` 活动日志；状态未变时为空操作。
- `invoke_bridge(command, params) -> Result<Value, HubError>`：2 秒超时，不重试；未连接为 `NotConnected`。
- `game_host(address) / game_join(address) -> Result<Value, HubError>`：包一层 `mp.host` / `mp.join`，参数是 app 拼好的 `address`，复用同一超时与权限判定。
- `take_events() -> Vec<HubEvent>`：std mpsc 排空；事件为 `SessionStarted`、`SessionEnded { pid, reason }`、`ClientEvent { pid, method, params }`（仅 `game.state` / `mods.loaded` / `mods.changed`）。

## 安装链路

桥是内置目录型模组，固定安装在 `<Plugins>/CuhubBridge`，不写 Store、Links 或 profile；内容库迁移固定跳过该目录（`AdoptSkip::BuiltIn`），不会把桥收进 Store 或改链接。

- `bridge_install_status() -> Result<BridgeInstallStatus, HubError>`：由 `plugins_dir()` 算出目标目录；只有 `CuhubBridge.Plugin.dll` 与 `CuhubBridge.Core.dll` 同时存在且处于同一启用/禁用状态才算已安装（文件名大小写须精确），版本取 Plugin；不写盘，不要求游戏关闭。
- `install_bridge(&Path) -> Result<BridgeInstallReport, HubError>`：先 `ensure_game_closed()`，`stage_archive` 解到 `<AppData>/Staging/<stamp>-<pid>`，`plan_package` 取 payload，只接受归档根或单一顶层目录下、名称精确匹配的 `CuhubBridge.Plugin.dll` 与 `CuhubBridge.Core.dll`（大小写不符、多层子目录或额外文件都报错）；用 `mods::read_identity` 校验 Plugin GUID 为 `dev.cuhub.bridge`，不符即报错且不写盘。目标路径被 `links.get` 命中时拒绝，避免破坏 profile。随后 `plan_payload` 以目标目录为 root（`owned_root` 也是它）生成计划，`apply_upgrade` 带备份执行，成功后写 `Bridge` 活动日志并返回版本、目标、文件数与备份；任何失败路径都清理 staging。
- 安装与更新都在应用内完成，界面入口在 Local Mods 底部；安装后的桥仍是普通本地模组行，启用、禁用、删除走现有模组操作。

## 线程

- 监听线程：非阻塞 `accept`（20ms 轮询），在自身线程完成握手；握手最长 2 秒，期间不接新连接。
- 每个会话一个读线程 + 一个心跳线程；写统一走会话写锁。
- 心跳每 10 秒发 `ping`，连续 3 次未回关闭会话；双方都可发 ping。
- `mp.state` 由 app 的后台轮询经 `invoke_silent` 发起（间隔至少 2 秒，只在会话存在时），连接线程不读写设置；权限按 `RwLock` 当前内容现取现判，运行中授权立即生效。
- 单会话：第二个连接在握手阶段被拒（同 pid 为 `duplicate_pid`，异 pid 为 `busy`）。

## 失败模式

| 现象 | 处理 |
|---|---|
| 游戏没运行 | 桥不启动，状态栏显示“游戏未运行” |
| 没装插件 / 客户端不连 | 桥在跑但没有会话，状态栏显示“未连接” |
| 端口被占用 | `PortInUse`，不 panic；状态栏显示端口被占用，app 其余功能不受影响 |
| token 不符 | 回 `bad_token` 后断开，记一行日志，状态里留最近失败 |
| 协议 major 不符 | 回 `proto_mismatch`，附双方 major；状态栏显示版本不符，成功握手后清空 |
| 两个游戏进程 | 后来的连接回 `duplicate_pid` / `busy` |
| 半截帧 / 超大帧 | 读错误或超限即断开会话并记日志，监听线程继续 |
| 心跳丢失 | 连续 3 次未回，关闭会话并记原因 |
| 游戏崩溃 | 读到 EOF，发 `SessionEnded`；游戏进程消失后 app 停止桥 |
| 请求超时 | `BridgeError::Timeout`，不重试 |
| 宿主崩溃 / 重启 | 客户端看到 EOF 后自行退避重连；握手回 `busy` / `duplicate_pid` 时延时重试，token / 协议不符则停止本次运行 |
| 两个宿主实例 | 第二个绑定失败为 `PortInUse`，不降级为只读 |

## 持久化

- 无发现文件、无锁文件；端口本身保证只有一个宿主。
- 权限在 `Settings.json` 的 `bridge.permissions`（appdata）。
- 审计进会话日志，不单独建文件；token 是源码常量，不进日志。

## 测试

`crates/core/tests/bridge_server.rs`：真实回环 TCP 假客户端，不需要游戏或 Wine，固定端口用进程内互斥串行化，覆盖

- 正常握手、命令表与 hello.ok 字段、`cmd.invoke` 往返（nonce 为字符串）；
- 坏 token、proto.major 不符、第二连接 `duplicate_pid` / `busy`、第二宿主 `PortInUse`；
- 握手失败进 `last_error`、成功后清空；
- 半截帧、超大帧、超时、心跳丢失、断线重连；
- 未知命令、默认权限拒绝、显式授权允许；运行中改权限对已连会话当场生效。

`cargo test -p cuhub-core` 全绿（342 单测 + 14 集成）。`cargo check --workspace --all-targets` 无警告。

## 未做

- `perms.changed`、游戏侧上行 evt（C# 尚未发）、多会话并行、重试/取消。
- 权限确认对话框与 W8 的设置界面。
- C# 插件真机加载与命令语义验证；AF_UNIX 传输未采用，保持 TCP 回环。

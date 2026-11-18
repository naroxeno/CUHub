# bridge（游戏内联动）

宿主侧的桥：监听回环端口、维护与单个游戏进程的连接、执行命令白名单。不负责游戏内的事，也不替 mod 干活。

## 实现状态（2026-11-19，实验性，不发布）

Rust 宿主 MVP 已落地：`proto.rs`（帧编解码）、`rendezvous.rs`（发现文件）、`command.rs`（命令表与权限）、`session.rs`（连接读写、心跳、请求等待）、`handshake.rs`（hello 校验）、`server.rs`（监听、单会话、锁、事件、审计），以及 `hub/bridge_host.rs` 上的 Hub 方法。`crates/core/tests/bridge_server.rs` 用回环 TCP 假客户端跑 13 个集成测试；`cargo test -p cuhub-core` 全绿。

游戏侧同时有可编译的 MVP：`CuhubBridge.Core`（协议常量、消息、帧、rendezvous、TCP 客户端）与 `CuhubBridge.Plugin`（探针、握手、心跳、`mp.*` 反射）。宿主按 C# MVP 的实际报文对齐：hello 是普通 `req`，失败用 `res.ok=false` 回 `bad_token` / `proto_mismatch` / `duplicate_pid`，`cmd.invoke` 的 `nonce` 与 `id` 都按字符串收发。`dotnet build -c Release` 通过，尚未真机加载。

联机状态经 CUCoreLib 的 `MultiplayerApi` 读，不直连 Casualties: Together。插件对 CUCoreLib 是运行时软依赖：全反射调用 `MultiplayerApi` / `MultiplayerBridge`，缺库时 `mp.*` 回 `unavailable`，插件本身照常加载。注意 CUCoreLib 文档里的 "bridge" 指它自己的 KrokMP 集成，与本模块同名不同物。

## 模块

| 文件 | 职责 |
|---|---|
| `bridge/proto.rs` | 长度前缀帧、JSON `Message`、`ErrorBody`、`MAX_FRAME` |
| `bridge/rendezvous.rs` | `CasualtiesHub/bridge.json` 的读写、校验、私有权限、删除 |
| `bridge/command.rs` | 15 条命令表、`Policy`、`Permission`、按 guid 解析权限、hello 快照 |
| `bridge/session.rs` | 帧读取、`cmd.invoke` 发送与超时、pending 表、心跳、上行事件过滤 |
| `bridge/handshake.rs` | 首帧 hello、token / proto / mods 校验、hello.ok / 拒绝 |
| `bridge/server.rs` | 监听 `127.0.0.1:0`、`bridge.lock`、rendezvous 写入、单会话、事件、审计 |
| `hub/bridge_host.rs` | `start_bridge` / `stop_bridge` / `bridge_status` / `invoke_bridge` / `take_events` |

## 职责

- 解析与写入 rendezvous 文件（`<prefix>/drive_c/ProgramData/CasualtiesHub/bridge.json`）。
- 监听 `127.0.0.1:0`，生成并持有本次会话的 token（32 字节随机数，64 位小写 hex）。
- 握手、心跳、帧编解码、命令表与权限判定。
- 把游戏侧上行事件翻成 `HubEvent::Bridge`，其余一概经 `Hub`。
- 把宿主侧的 `cmd.invoke` 编码、发送、超时、审计。

## 边界

- 不做 UI：状态只发事件，不写 signal；`take_events` 目前无消费者。
- 不碰凭据：`nexus_auth` 的内容不进桥。
- 不做文件读写通道：命令 handler 在游戏侧，宿主不替它读写磁盘。
- 不去猜 prefix：`start_bridge` 的 `program_data` 由调用方给出；找不到就明确报“桥不可用”，不遍历文件系统。
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
- 授权变更不在握手里做；协议要求的 `perms.changed` 尚未实现。

## Hub 接口与事件

- `start_bridge(program_data) -> Result<BridgeStatus, HubError>`：重复调用返回现有状态。
- `stop_bridge()`：清会话、删 rendezvous、释放锁；`bridge_status()`：`running`、`endpoint`、`program_data`、`session`。
- `invoke_bridge(command, params) -> Result<Value, HubError>`：2 秒超时，不重试；未连接为 `NotConnected`。
- `take_events() -> Vec<HubEvent>`：std mpsc 排空；事件为 `SessionStarted`、`SessionEnded { pid, reason }`、`ClientEvent { pid, method, params }`（仅 `game.state` / `mods.loaded` / `mods.changed`）。

## 线程

- 监听线程：非阻塞 `accept`（20ms 轮询），在自身线程完成握手；握手最长 2 秒，期间不接新连接。
- 每个会话一个读线程 + 一个心跳线程；写统一走会话写锁。
- 心跳每 10 秒发 `ping`，连续 3 次未回关闭会话；双方都可发 ping。
- 命令由 `Hub` 线程发起，经 std mpsc 等待结果；连接线程不读写设置，权限用启动时传入的快照。
- 单会话：第二个连接在握手阶段被拒（同 pid 为 `duplicate_pid`，异 pid 为 `busy`）。

## 失败模式

| 现象 | 处理 |
|---|---|
| 没装插件 / 客户端不连 | 正常状态，不报错 |
| token 不符 | 回 `bad_token` 后断开，记一行日志，不透露细节 |
| 协议 major 不符 | 回 `proto_mismatch`，附双方 major |
| 两个游戏进程 | 后来的连接回 `duplicate_pid` / `busy` |
| 半截帧 / 超大帧 | 读错误或超限即断开会话并记日志，监听线程继续 |
| 心跳丢失 | 连续 3 次未回，关闭会话并记原因 |
| 游戏崩溃 | 读到 EOF，发 `SessionEnded`，保留 rendezvous 文件 |
| 请求超时 | `BridgeError::Timeout`，不重试 |
| 宿主崩溃 | 客户端看到 EOF 自行退避重连；残留 rendezvous 下次启动覆盖 |
| 两个宿主实例 | `bridge.lock` 的 `try_lock` 只允许一个；第二个 `start_bridge` 返回 `Locked`，不降级为只读 |

## 持久化

- rendezvous 文件在调用方给的 `program_data` 里，0600，正常停止时删除。
- `bridge.lock` 在 `<appdata>`，只做单宿主互斥，不写内容。
- 权限在 `Settings.json` 的 `bridge.permissions`（appdata）。
- 审计进会话日志，不单独建文件；token 只出现在 rendezvous 文件里，不进日志。

## 测试

`crates/core/tests/bridge_server.rs`：真实回环 TCP 假客户端，不需要游戏或 Wine，覆盖

- 正常握手、命令表与 hello.ok 字段、`cmd.invoke` 往返（nonce 为字符串）；
- 坏 token、proto.major 不符、第二连接 `duplicate_pid` / `busy`、双宿主 `Locked`；
- 半截帧、超大帧、超时、心跳丢失、断线重连；
- 未知命令、默认权限拒绝、显式授权允许。

`cargo test -p cuhub-core` 全绿（301 单测 + 13 集成）。`cargo check --workspace --all-targets` 无警告。clippy 基线里既有未提交改动仍有告警，与桥无关。

## 未做

- UI 的事件消费与“桥：未连接 / 已连接”状态展示、权限确认对话框。
- `perms.changed`、游戏侧上行 evt（C# 尚未发）、多会话并行、重试/取消。
- C# 插件真机加载与命令语义验证；AF_UNIX 传输未采用，保持 TCP 回环。

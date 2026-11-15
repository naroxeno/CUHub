# bridge（游戏内联动）

宿主侧的桥：监听回环端口、维护与每个游戏进程的连接、执行命令白名单。不负责游戏内的事，也不替 mod 干活。

## 实现状态（2026-11-15，实验性，不发布）

只落地两块，且都无副作用：帧编解码（`proto.rs`）与 rendezvous 文件（`rendezvous.rs`），11 个单测覆盖往返、半截帧、超大帧、未知字段、坏文件与校验拒绝。**没有**监听、会话、握手、命令表、审计，也没有游戏侧运行时：`CuhubBridge.Core` 目前只是协议常量加 `Implemented = false`，`CuhubBridge.Plugin` 尚未建立（它需要 BepInEx 5 与游戏 `Managed` DLL 才能编译，本机没有）。

联机状态大概率要经 CUCoreLib 的 `MultiplayerApi` 读，而不是直连 Casualties: Together，等于插件对 CUCoreLib 有硬依赖；`mp.*` 的实现方式与命令 id 是否统一加 `cuhub.` 前缀，都留到真机接入时定。注意 CUCoreLib 文档里的 "bridge" 指它自己的 KrokMP 集成，与本模块同名不同物。

## 职责

- 解析与写入 rendezvous 文件（`<prefix>/drive_c/ProgramData/CasualtiesHub/bridge.json`）。
- 监听 `127.0.0.1:0`，持有本次会话的 token。
- 握手、心跳、帧编解码、命令表与权限判定。
- 把游戏侧状态翻成 `HubEvent::Bridge`，其余一概经 `Hub`。
- 把宿主侧的 `cmd.invoke` 编码、发送、超时、审计。

## 边界

- 不做 UI：状态只发事件，不写 signal。
- 不碰凭据：`nexus_auth` 的内容不进桥。
- 不做文件读写通道：命令 handler 在游戏侧，宿主不替它读写磁盘。
- 不去猜 prefix：prefix 由 `steam` 或用户设置给出；找不到就明确报“桥不可用”，不遍历文件系统。

## 线程

- 监听线程：只 `accept` 与起读线程，不落盘。
- 每连接一个读线程：解帧 → 校验 → 投递；写走同一连接的写锁。
- 命令从 `Hub` 发起，经 `crossbeam` 送到连接线程；结果回到 `Hub`，再发事件。
- 连接线程不读设置、不写设置；权限从 `Hub` 传入的快照里查。

## 失败模式

| 现象 | 处理 |
|---|---|
| 没装插件 / 客户端不连 | 正常状态，不报错；状态栏显示“桥：未连接” |
| token 不符 | 一个连接一个日志行，不回详情，不触发退避 |
| 协议 major 不符 | 拒绝并给出双方版本 |
| 游戏崩溃 | 读到 EOF，会话标死，保留 rendezvous 文件供下次 |
| 宿主崩溃 | 客户端看到 EOF 自行退避重连 |
| 两个宿主实例 | `<appdata>/bridge.lock` 的 flock 只允许一个持有者写 rendezvous 文件；第二个实例降级为只读 |

## 持久化

- 运行时文件在 prefix 里，不进 appdata。
- 权限在 `Settings.json` 的 `bridge.permissions`（用户数据，在 appdata）。
- 审计进会话日志，不单独建文件。

## 测试

`bridge-selftest`：Rust 冒充客户端，跑握手、坏 token、超大帧、未知字段、重复 nonce、半关闭、重连、双宿主抢锁。不需要 Wine，因此三端 CI 都能跑。

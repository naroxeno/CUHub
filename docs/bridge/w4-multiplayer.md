# 桥接入联机（W4）实现方案

2026-11-20。范围：把桥已有的 `mp.state` / `mp.host` / `mp.join` 接进 Multiplayer 页，完成 W4。W2/W3 已接线，见 `docs/FEATURES.md`。

## 0. 与“桥归模组安装机制”的关系

- 这是发布前的硬前置，和本方案分开做：桥最终要作为普通目录型模组装进 Store、写 Links 与 profile，卸载/更新/修复都走模组链路。
- 当前固定 `Plugins/CuhubBridge`、不写 Store/Links/profile，以及 adopt 的 `AdoptSkip::BuiltIn` 都只是临时兜底，不是最终设计。
- 本方案不依赖桥装在哪；先做控制，不等于认可现在的固定安装。发布前必须另开任务完成托管重构，并把 adopt 特判删掉。

## 1. 结论

- 控制权限默认关闭。Multiplayer 顶部桥卡片增加一个“允许联机控制”开关，只授权 `dev.cuhub.bridge` 的 `mp.host` / `mp.join`，不碰其它命令。
- 授权后：房主在桥会话就绪时自动开房一次；客人在隧道连上房主后自动加入，最多 3 次退避重试。两边都保留手动按钮兜底。
- 房主地址用 `10.144.144.1:<port>`；客人用 `TunnelState::Connected { virtual_ip, port }` 里的房主地址。端口取房间当前端口。
- `mp.leave` / `mp.kick` 暂不接：C# 侧声明了但未实现；离开仍走现有 EasyTier 的“离开房间”。玩家列表也没有 CUCoreLib 公开 API，W3 保持只读 flag。
- QuickTest 是 CUCoreLib 的测试入口，未真机验证。自动逻辑必须满足：失败即停、可手动重试、可一键关闭，不能无限重试。

## 2. 现状与缺口

- `crates/app/src/pages/multiplayer.rs:55` 的 `bridge_card` 只展示 `BridgeSnapshot.mp`；`room_card`（:367）只有角色、房间码、peer 和离开，没有游戏内开房/加入。
- `crates/app/src/ui/multiplayer.rs:24` 的 `MultiplayerUi` 只有 EasyTier 的 host/join/leave；没有桥命令的 busy、错误和确认态。
- `crates/core/src/hub/bridge_host.rs:70` 的 `bridge_snapshot` 每 2 秒轮询 `mp.state`；`crates/app/src/ui/bridge.rs:14` 的 `watch_bridge` 只消费快照，没有命令通道。
- `crates/core/src/bridge/server.rs:142` 的 `ServerShared.permissions` 是启动时快照；当前没有修改入口。`command.rs:157` 对 control 默认 `Denied`。
- C# 侧 `MpBackend.Start`（`mods/cuhub-bridge/CuhubBridge.Plugin/MpBackend.cs:45`）已能 host/join：address 必填，失败返回 `unavailable` 和原因；`CommandRouter.cs:55` 已校验 address 非空、无空白、≤128。
- 现有文档 `docs/bridge/plan.md:85` 写的是“房主点按钮、客人自动 join”；本方案把房主也改成自动，但保留按钮。

## 3. 权限：一个开关，活更新

### 3.1 数据

- 继续用 `Settings.json` 的 `bridge.permissions`（`crates/core/src/bridge/command.rs:106`）。
- key：`dev.cuhub.bridge`（常量在 `crates/core/src/bridge.rs:3`）；命令只写 `mp.host`、`mp.join`。开=Granted，关=Denied。
- 不要默认给 `mp.leave` / `mp.kick` 授权；等 C# 实现后再加。

### 3.2 接口

- `crates/core/src/bridge/server.rs:142`：`ServerShared.permissions` 改成 `RwLock<Permissions>`；`:221` 的 `invoke_with` 读锁取快照；新增 `BridgeServer::set_permissions(&self, Permissions)`。
- `crates/core/src/hub/bridge_host.rs` 新增：
  - `Hub::bridge_control_enabled(&self) -> bool`
  - `Hub::set_bridge_control(&mut self, enabled: bool) -> Result<(), HubError>`
  内部改 `self.settings.bridge`，调 `save_settings`（`crates/core/src/hub.rs:528`），再对运行中的 server 调 `set_permissions`；写 `Bridge` 日志（旧值、新值、来源 UI）。
- `BridgeSnapshot` 增加 `control: bool`，UI 不用自己读 Settings。

### 3.3 UI

- 开关放在 Multiplayer 的 `bridge_card`，不放全局设置页；W8 的完整命令矩阵以后再做，避免这次扩大范围。
- 打开时显示一句确认文案：只允许 CUHub 在游戏内开房/加入，不影响其它桥命令。关闭立即生效。
- 未授权时，游戏内开房/加入按钮禁用，提示先打开开关；自动状态机也不启动。

## 4. 状态机

输入：`role`、`TunnelState`、`BridgeSnapshot { status.session, mp, control }`、端口。输出：一次 `mp.host` / `mp.join`。

| 状态 | 条件 | 动作 |
|---|---|---|
| 不可用 | `mp.available == false` | 禁用按钮，提示 CUCoreLib 未加载 |
| 未连接 | `status.session.is_none()` | 禁用按钮，提示等游戏连上桥 |
| 未授权 | `control == false` | 显示开关，不自动调用 |
| 房主待开 | Host + session + available + `!mp.running` | 自动 `mp.host` 一次；地址 `10.144.144.1:<port>`；不要求 Tunnel Connected |
| 客人待加入 | Guest + session + Connected + available + `!mp.running` | 自动 `mp.join`；地址 `<virtual_ip>:<port>`；最多 3 次，退避 0/3/8 秒 |
| 已开房 | `mp.running && mp.host` | 显示“已开房”，停止自动 |
| 已加入 | `mp.running && mp.client && !mp.host` | 显示“已加入”，停止自动 |
| 失败 | 命令返回错误 | 停止自动，显示原因，给“重试”按钮 |

- 每个桥会话（`status.session.pid`）只自动执行一轮；pid 变化或隧道断开重连后重置。
- `mp.running` 已经为 true 时不调用；如果状态和角色冲突（房主却 client、客人却 host），只显示状态，不自动纠偏。
- 客人自动只用于“隧道已连上房主”的情况；房主可能还没开房，所以允许重试，但必须封顶。收到 `Denied` 或 `InvalidParams` 立即停止，不重试。
- 房主不自动重试：失败多半是 CUCoreLib/QuickTest 不可用，重试无意义，交给按钮。

## 5. 代码落点

- 新增 `crates/app/src/ui/mp_control.rs`：
  - `watch_mp_control`：每 2 秒读 `ui.bridge` 与 `ui.mp`，满足条件时在 worker 里锁 Hub 调 `invoke_bridge`；不阻塞主线程。
  - `mp_request_game_control(host)`、`mp_confirm_game_control`、`mp_cancel_game_control`、`mp_retry_game_control`、`mp_toggle_bridge_control`。
  - 地址拼装抽成纯函数，单测覆盖 host/guest。
- `crates/app/src/ui/multiplayer.rs`：
  - `MpBusy` 增加 `Game`；新增 `game_error: Signal<String>`、`game_confirm: Signal<Option<bool>>`。
  - 不要复用现有 `error`（它是 `&'static str` 的隧道错误），桥返回的原文单独放 `String`。
- `crates/app/src/pages/multiplayer.rs`：
  - `room_card` 在房间码下面加“游戏内房间”区块：状态、地址+复制、主按钮、错误、重试、确认块。
  - 房主按钮文案“在游戏内开房”，客人“加入游戏内房间”；确认块写清地址和“实验性功能”。
  - 现有“离开房间”保持在最下面，不改语义。
- `crates/app/src/ui/state.rs:100` 的 `bridge` signal 不动；`watch_bridge` 不动。两个 2 秒循环各自读快照/发命令，Hub 锁自然串行。
- 命令超时沿用 `invoke_bridge` 的 2 秒；忙时禁用按钮，不叠加请求。

## 6. 失败与兜底

- 任何失败都显示 C# 返回的原因（已做长度/空白校验），并保留手动地址：
  - 房主：`10.144.144.1:<port>`
  - 客人：`<virtual_ip>:<port>`
- 提供“复制地址”，提示可在游戏内联机菜单手动填。桥不可用时页面不阻塞，也不影响 EasyTier。
- `Denied` 显示“先开启联机控制”；`NotConnected` 显示“游戏未连上桥”；`Unavailable` 显示原因（常见是 CUCoreLib 未加载或 QuickTest 返回 false）。
- 客人 3 次失败后停止，不再后台重试；用户点“重试”才继续。房主失败不自动重试。

## 7. 验证

- 单元测试：
  - `Hub::set_bridge_control` 只改 host/join 两条，开/关都持久化，`BridgeSnapshot.control` 跟随。
  - `PortWatch`：350ms 采样、连续 2 次确认、6 秒窗口；抖动不换端口，稳定才换。
  - `choose_host_port`：上报 > 手填 > 默认 25565，0 视为未填；非法与占用返回错误，不静默换号。
  - `mp.state` 宽容解析：缺 port、null、非整数、越界、客人侧都当未检测，不报错。
  - 地址拼装与状态门控：host/guest、端口边界，`GamePhase` 组合。
- 集成测试：`tests/bridge_server.rs` 14 条（握手、权限活更新、超时、心跳、超大帧、重连）。
- C#：`just build-bridge` / `just package-bridge`，`dotnet build -c Release` 0 警告；插件版本 0.1.1。
- 真机（必须，等 Naro 约到搭档后两机执行）：用例编号、步骤与取证见 `docs/bridge/w4-tickets/README.md`，裁定与偏差见 `docs/bridge/w4-tickets/decisions.md`；R1–R11 对应关系：
  - R1 房主先开房、客人后进；R2 客人先等待、房主隧道后上线自动连上。
  - R3 桥卡片开关（默认关、打开立即生效、关闭后不再自动）。
  - R4 QuickTest 失败四组（缺 CUCoreLib、端口占用、名称过短、非法端口）。
  - R5 游戏退出与重开；R6 桥断开与重连。
  - R7 游戏内建房 7790 / 24444 的端口检测与来源标签。
  - R8 游戏内直接开房不重复下发；R9 换房 / 重进清理。
  - R10 房主退出后客人侧；R11 房主换端口后客人侧（提示「房主已换房，请用房间码重进」）。
- 回归：`cargo fmt --all -- --check`、`cargo test -p cuhub-core`、`cargo test -p cuhub --all-targets`、`just build-bridge`，不新增警告；批次 F 自动部分见 `docs/TEST-PLAN.md` §8.1。

## 8. 文档与文案

- `docs/FEATURES.md:387` W4 改为“默认关闭、桥卡片开关、房主自动一次、客人最多 3 次、手动兜底”；:395 实现状态同步。
- `docs/bridge/plan.md:85-99` 更新房主/客人流程与 M3 验收；风险保留“QuickTest 可能只是测试入口”。
- `docs/bridge/protocol.md:116-124` §7 补一句：授权在运行中立即生效，不需要重连。
- `docs/core/bridge/overview.md` 补权限活更新与 W4 状态机；`docs/app/overview.md` 补页面入口。
- `locales/app.yml` 增加 `multiplayer.bridge_control*`、`multiplayer.game_*`、`multiplayer.busy_game`，中英日三套。

## 9. 实施顺序

1. core 权限活更新 + `BridgeSnapshot.control` + 测试。
2. UI 开关 + 手动开房/加入 + 错误显示；先不自动，真机跑通 QuickTest。
3. 打开自动状态机（房主一次、客人 3 次），补状态门控测试。
4. 文档、文案、真机回归；W4 才算完成。

## 10. 待拍板

1. 开关位置：桥卡片（推荐，W4 内聚）还是设置页（W8 更完整）。
2. 自动范围：房主自动 + 客人自动（推荐）；还是只自动房主、客人手动。
3. 客人重试次数/退避：3 次、0/3/8 秒（推荐）是否合适。
4. 桥的 Store/Links/profile 托管重构是否作为 W4 发布前置单独排期（推荐是）。
5. QuickTest 的真实语义与地址绑定需要一次真机确认；确认前不要把自动逻辑当稳定能力。

## 11. 非目标

- 不做玩家列表、房间内踢人、`mp.leave`。
- 不做存档/皮肤命令。
- 不改协议字段、不改 C# 命令表、不做局域网发现。
- 不把桥当通用 RPC，不绕过权限。

## 12. 进度（2026-11-20）

- 步骤 1 完成：`ServerShared.permissions` 改为 `RwLock<Permissions>`，`invoke_with` 每条命令现取现判；新增 `BridgeServer::set_permissions`；`BridgeSnapshot` 增加 `control`；新增 `Hub::bridge_control_enabled` 与 `Hub::set_bridge_control(bool)`（只改 `dev.cuhub.bridge` 的 `mp.host` / `mp.join`，写回 `Settings.json`，桥在运行时立即生效）。
- 测试：`hub/bridge_host.rs` 两条纯函数单测；`tests/bridge_server.rs` 新增「默认拒绝 → 运行中授权 → 同一会话允许」；`cargo test -p cuhub-core` 342 单测 + 14 集成全绿；`docs/core/bridge/overview.md` 已同步。
- 步骤 2–3 完成（未提交）：
  - app 新增 `crates/app/src/ui/mp_control.rs`：`GamePhase`（NoSession / Unavailable / NoControl / WaitingTunnel / Ready / Hosting / Joined）、`game_phase`、`game_status_key`、`game_address`、`game_error_text`、`stops_auto` 与 `watch_mp_control`（2 秒轮询；`status.session.pid` 变更重置门控；客人隧道 `Connected` 端点变更重置 attempts；房主只自动一次；客人最多 3 次、退避 0/3/8 秒；`denied` 与 `invalid_params` 立即停；确认对话框打开或其它任务忙碌时不自动）。
  - 桥卡片新增「游戏内控制」开关（`mp_toggle_bridge_control`）；房间卡新增「游戏内房间」区块：状态文案、地址与复制、手动开房/加入（两段式确认）、重试、错误行、`MpBusy::Game`。
  - core 新增 `Hub::game_host` / `Hub::game_join`，包一层 `mp.host` / `mp.join`，参数是 app 拼好的 `address`。
  - 文案：`multiplayer.bridge_control*`、`multiplayer.game_*`、`multiplayer.busy_game`，en / zh-CN / ja。
- 测试：`mp_control.rs` 新增 10 条纯函数单测（会话/可用性/权限门控、房主与客人状态、地址拼装与端口边界、状态键）；`cargo test -p cuhub --all-targets` 15 条全过；`cargo fmt --all` 干净；`cargo check -p cuhub --all-targets` 无警告。
- 剩余：真机回归（§7）；桥托管重构（§0）作为发布前置。文档已同步：`docs/FEATURES.md`、`docs/bridge/plan.md`、`docs/bridge/protocol.md`、`docs/core/bridge/overview.md`、`docs/app/overview.md`。
- §10 决议（2026-11-20，Naro）：开关在桥卡片；房主与客人都自动；客人 3 次、0/3/8 秒；桥托管重构作为发布前置单独排期；QuickTest 保持实验性，真机确认前不当稳定能力。

## 13. 进度（2026-11-21，端口自动检测与交接）

- core：`MpState.port` 只保留 `running && server` 的上报值；新增 `Hub::game_room_port()`、`multiplayer::port`（`choose_host_port`：上报 > 手填 > 默认 25565，`PortSource`、`PortWatch` 350ms×2 / 6s 窗口、`PortUpdate`）。`Hub::start_host` / `join_room` 改走该选择；新增 `restart_host_port` 与阻塞式 `reconcile_room_port`；`start_game_host()` 要求房主隧道，下发 `mp.host` 后回读端口，与隧道端口不一致返回 `HubError::GamePort`。
- app：开房/加入前显示端口来源与「重新检测」，手填与上报不一致时可一键采纳；`game_phase` 把 `mp.server` 视为 `Hosting`；`watch_mp_control` 增加会话进入/换房/退出交接、客人侧房主离开或换端口提示、房主 reconcile 三态与检测缓存；新增 `MpBusy::Port` 与三语文案。
- 房主换端口后客人旧隧道目标失效：采用低成本方案，提示「房主已换房，请用房间码重进」，不自动重新发现。
- 验证：`cargo test -p cuhub --all-targets` 25 条全过（其中 mp_control 15 条）；`cargo check -p cuhub --all-targets` 无警告。

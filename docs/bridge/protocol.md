# 游戏内联动协议（CU Bridge Protocol）

cuprum（Rust，宿主）与游戏内 BepInEx 插件（C#，客户端）之间的线协议，以及第三方 CU mod 接入时依赖的契约。
实现可以换，这份契约不能随手改；改了就要抬 `proto` 主版本。

命名提示：CUCoreLib 文档里的 "bridge" 指它对 KrokMP 的集成，与本协议的 `CU Bridge` 无关；引用时写全名。

## 1. 拓扑与职责

- **宿主（host）= cuprum**：监听方，持有真相（装了哪些模组、开关、设置），决定哪些命令被允许。
- **客户端（guest）= 游戏进程**：连接方，每次启动一条连接；进程内所有 mod 共用这一条（由 `CuprumBridge.Core` 收口），不允许各 mod 自己开。
- 宿主同时只服务一个游戏进程；第二个连接在握手阶段被拒（同 pid 为 `duplicate_pid`，异 pid 为 `busy`）。
- 宿主不在时，客户端必须降级为无桥运行：不抛异常、不阻塞主线程、不往游戏目录之外写东西。

## 2. 端点与生命周期

宿主固定监听 `127.0.0.1:41297`；客户端直接连这个端口，不读发现文件、不写游戏 prefix。固定端口是为了避开跨 Wine/Proton 的发现路径（`%ProgramData%`、prefix、bottle 三种推导都不可靠），loopback TCP 是目前唯一已验证可用的本机传输。

- 宿主只在检测到游戏进程运行，或用户从 cuprum 点启动游戏时开始监听；游戏退出后停止并断开。
- 端口被占用时宿主不启动桥，把失败留在状态里（状态栏显示端口被占用），不 panic、不静默换端口。
- 客户端连不上 → 退避重试（1/2/5/10/30 秒封顶），只在第一次失败时写一条日志。
- 客户端不得因为桥的任何失败影响游戏启动与主循环。

token 不再是运行时生成的文件内容：Rust 与 C# 两端各有一个相同的 64 位小写十六进制常量（`crates/core/src/bridge/handshake.rs` 的 `TOKEN`、`CuprumBridge.Core/Transport.cs` 的 `Transport.Token`），改一处必须同时改另一处。它只挡住误连（例如连到别的旧版本或别的程序），不是安全边界；边界是只监听回环地址与握手的字段校验。

## 3. 帧

`u32` 小端长度（不含自身 4 字节）+ UTF-8 JSON 对象。长度上限 1 MiB，超限直接断开并记一行日志。

## 4. 消息

| kind | 形态 | 说明 |
|---|---|---|
| `req` | `{v, kind:"req", id, method, params}` | 必须回 `res` |
| `res` | `{v, kind:"res", id, ok, result?, error?}` | `ok:false` 时 `error:{code, message}` |
| `evt` | `{v, kind:"evt", method, params}` | 无 id，不回复 |

- `id` 由发起方自增，双向各一套空间，响应必须回同一个 id。
- 未知 `kind` / `method` / 字段一律忽略（前向兼容）；未知 `kind` 不得回 `res`。
- 同 `id` 在 60 秒内重复到达视为重发，回上次结果，不重复执行。

## 5. 方法

### 5.1 握手

`guest → host: hello`（普通 `req`，`method:"hello"`）

| 字段 | 含义 |
|---|---|
| `token` | 固定共享常量，常量时间比较；不符即拒 |
| `proto` | `{major, minor}`；`major` 不等即拒 |
| `caps` | 客户端能理解的扩展方法名列表 |
| `game` | `{pid, unity_version, bepinex_version}` |
| `mods` | `[{guid, version}]`，进程内已加载的模组 |

`host → guest: hello.ok`：`res` 的 `result` 为 `{host_version, caps, commands, perms}`（`ok:true`）。

`commands` 是宿主已知的命令表，`perms` 是每条命令当下的授权（`granted` / `denied` / `ask`）。授权不在握手里改，只能由用户在 cuprum 里改，改完用 `evt: perms.changed` 通知。

拒绝：先回 `res.ok:false` 再断开；`bad_token`（message 只写 token rejected，不回详情）、`proto_mismatch`（附带双方 major）、`duplicate_pid`、`busy`（已有会话）。宿主把最近一次握手失败留在桥状态里，状态栏据此显示坏 token / 版本不符 / 忙；成功握手后清空。客户端收到 `busy` / `duplicate_pid` 后按自身退避策略重试；`bad_token`、`proto_mismatch` 不在本次运行内重试。

### 5.2 心跳

`ping` 是 `req`，对端用 `res` 回；双向各发，10 秒一次；连续 3 次未回则断开重连。

### 5.3 状态上报

| 方法 | 方向 | 内容 |
|---|---|---|
| `game.state` | guest → host | `{screen: menu\|loading\|in_world, session?: {...}}` |
| `mods.loaded` | guest → host | 已加载模组与加载时间，用于显示“这个模组真的起来了” |

MVP 尚未实现这两个上报，宿主收到未知 `evt` 忽略。

### 5.4 命令

`host → guest: cmd.invoke {nonce, id, params}`，客户端用 `res` 回：

| error.code | 何时 |
|---|---|
| `unknown_command` | `id` 不在命令表 |
| `denied` | 权限不足或用户未授权 |
| `invalid_params` | 参数不符合 `schema` |
| `busy` | 同一条命令已有一次在飞，或目标资源被占用（`mp.host` 的 UDP 端口已被占用） |
| `unavailable` | 当前游戏状态不允许（如不在世界内） |
| `timeout` | 客户端内部超时（默认 2 秒） |

- `nonce` 由宿主生成，客户端在 60 秒窗口内去重。
- 每条命令单飞；宿主侧超时 2 秒即视为失败，不自动重试（重试由用户点第二次触发）。

### 5.5 通知

`host → guest: evt: config.changed {scope}`；`guest → host: evt: mods.changed`。

## 6. 命令表

命令 id 由游戏侧 mod 声明，宿主只能调用声明过的 id。命名 `{mod}.{动作}`，一旦发布不得改语义。

| 命令 | 策略 | 说明 |
|---|---|---|
| `mp.state` | read | 房间、角色、玩家列表 |
| `mp.host` / `mp.join` / `mp.leave` | control | 开房 / 加入 / 离开 |
| `mp.kick` | mutate | 踢人，逐次确认 |
| `save.list` / `save.current` | read | 槽位列表、当前槽位 |
| `save.save` | mutate | 写档，覆盖前需确认 |
| `save.load` | mutate | 读档，当前进度丢失，必须逐次确认 |
| `save.backup` / `save.restore` | mutate | 备份 / 还原 |
| `skin.current` / `skin.list` | read | 当前皮肤与可选项 |
| `skin.apply` | control | 热切换 |
| `skin.reload` | control | 重新扫描 CustomSprites |

参数校验用 JSON Schema 子集：`type`（`string`/`integer`/`boolean`/`object`）、`required`、`enum`、`minimum`/`maximum`、`maxLength`。两端各校验一次：宿主不信客户端，客户端也不信宿主。

### 6.1 `mp.state` 的 result

只读快照；字段只增不删，宿主遇到不认识的字段应忽略。

| 字段 | 类型 | 含义 |
|---|---|---|
| `available` | bool | CUCoreLib 的 `MultiplayerApi.IsAvailable` |
| `running` | bool | 联机是否在跑（`IsRunning`） |
| `connected` | bool \| null | Together 的 `IsConnected`，区分正在拨号和已连接。旧插件省略此字段 |
| `client` / `server` / `host` | bool | 角色（`IsClient` / `IsServer` / `IsHost`） |
| `bridge_receivers_installed` | bool | CUCoreLib 的自定义通道接收器是否装好 |
| `krokmp_version` | string \| null | CT 版本（`System.Version.ToString()`），读不到为 `null` |
| `port` | integer \| null | 实际监听端口（`u16`）；只有 `running && server` 时才可能非空 |

`port` 读的是 `Together.Net.Transport`（public static 属性）→ `Together.TransportLiteNetLib.port`（public readonly `ushort` 字段），不是常量 7790，也不是从 `/proc` 反推。语义按角色分：房主侧是**监听端口**，客人侧的 `port` 是目标端口、本机并不监听，所以只在房主侧给值，客人恒 `null`。Steam 传输没有这个字段，也是 `null`。

"没开房"与"读不出来"要分开：没开房是 `port: null`（正常快照）；Together 类型/字段缺失时 `port` 同样为 `null`，快照不因探测失败而整条失败——只有 CUCoreLib 本身没加载，`mp.state` 才回 `unavailable`。

### 6.2 `mp.host` / `mp.join` 的 result

`params`：`address`（`host:port`，≤128 字符，必填）、`name`（≤64 字符，可选；给了就先写 `UIMainMenu.USERINPUT_NAME`）。`mp.host` 的 `name` 少于 3 字符回 `invalid_params`，与 CT 自己的 `net_name_too_short` 一致。

成功时 `result`：

| 字段 | 类型 | 含义 |
|---|---|---|
| `started` | bool | 恒 `true`；失败走 `error` |
| `host` | bool | 本次是开房（`true`）还是加入（`false`） |
| `address` | string | 请求里的 `address`，原样回（只 trim，不改写） |
| `port` | integer | 解析出的端口（`u16`） |

`port` 用 CT 自己的规则解析，与 `Net.TryParseIPPORT` 等价：`[v6]:port` 取括号后的端口；其它情况只有"点号在冒号前"才按 IPv4 取最后一个冒号后的端口；否则整串当 IPv6、端口回落 `7790`。所以**无点号的主机名带端口（如 `localhost:24444`）会被 CT 当成 IPv6，端口是 7790**——这是游戏的既有怪癖，插件照着来、不修正；宿主想连这种地址应当自己补点号或直接用 IP。端口 `0` 直接 `invalid_params`：CT 会把 0 交给 `NetManager.Start(0)` 随机挑端口，隧道追不上。

成功回包前回读确认：`Together.Net.IsRunning` 为 `true`，`IsServer`（host）/ `IsClient`（join）与请求一致，`Transport` 非空；房主侧还要求 `Transport.port` 等于请求端口。任一项不符回 `unavailable`，message 写明哪一项没对上，不假装成功。

客人的 `started` 只确认游戏已启动连接，入房状态以随后 `mp.state.connected` 为准。客机已有活动游戏会话时，重复调用 `mp.join` 回 `busy`，由玩家先在游戏内退出原会话。

失败映射：

| error.code | 何时 |
|---|---|
| `invalid_params` | `address` 缺失/超长/含空白或控制字符、端口解析失败、端口 0、`name` 少于 3 字符或超长 |
| `busy` | 目标 UDP 端口已被占用（`AddressAlreadyInUse`）；或调用后返回失败且该端口正被占用 |
| `unavailable` | 缺 CUCoreLib/Together、反射成员缺失、`TryStartLocalQuickTest*` 返回 `false`、回读不一致、反射抛异常 |

`busy` 在调用前后各探一次 UDP 绑定：调用前探到端口被占就不调（CT 的 `OnWantToConnect` 会先 `ShutdownReset`，端口被占时照调会把已经在跑的房间拆掉）；调用后返回 `false` 且端口被占才算占用。

CT 的失败原因只有本地化的 `multiplayerStatusMessage`。插件把它压成一行、截断后拼进 message，只作可读线索；判错不看这个字符串，也永不按它分支。

## 7. 权限

| 档 | 默认 | 执行条件 |
|---|---|---|
| `read` | 允许 | 连接即可读 |
| `control` | 关闭 | 用户在设置里显式开启 |
| `mutate` | 关闭 | 每次执行前两段式确认；可选“本次游戏运行内允许” |

- 授权持久化在 `Settings.json` 的 `bridge.permissions`，按 `mod guid` + 命令 id 记。
- 新连接、新命令出现时一律按默认档处理，不继承上次的临时授权。
- 任何授权变更写日志（旧值、新值、触发来源）。
- 授权在运行中立即生效：宿主下一条命令就按新值判定，不需要重连；hello.ok 的 `perms` 仍是握手时的快照，之后不重发。

## 8. 安全边界

- 只监听 `127.0.0.1:41297`；不监听 `0.0.0.0`，不做局域网发现。
- token 是两端共享常量，只防误连，不承担认证；新增桥字段仍以本文件为白名单。
- 游戏侧发来的一切按不可信输入处理：帧上限、字段长度、id 白名单、数值范围。
- 凭据（Nexus key、GitHub token）永不进入桥；桥上字段以本文件为白名单，新增字段要过一遍这里。
- 宿主不得把桥当作文件系统或进程控制通道：不做任意路径读写、不做进程注入、不做通用 RPC。
- 客户端不得在桥上执行宿主给的路径或代码；handler 只做它自己声明过的事。

## 9. 版本

- `proto.major` 变更 = 破坏性，两端都拒连并给出可读原因。
- `proto.minor` 变更 = 只增字段，旧端忽略。
- 命令表只增不删；删命令等同破坏性变更。
- 第三方 mod 的接入面是 `CuprumBridge.Core` 的公开 API + 本文件，不是 `CuprumBridge.Plugin` 的内部实现。

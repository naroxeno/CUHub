# 游戏内联动协议（CU Bridge Protocol）

cuhub（Rust，宿主）与游戏内 BepInEx 插件（C#，客户端）之间的线协议，以及第三方 CU mod 接入时依赖的契约。
实现可以换，这份契约不能随手改；改了就要抬 `proto` 主版本。

命名提示：CUCoreLib 文档里的 "bridge" 指它对 KrokMP 的集成，与本协议的 `CU Bridge` 无关；引用时写全名。

## 1. 拓扑与职责

- **宿主（host）= cuhub**：监听方，持有真相（装了哪些模组、开关、设置），决定哪些命令被允许。
- **客户端（guest）= 游戏进程**：连接方，每次启动一条连接；进程内所有 mod 共用这一条（由 `CuhubBridge.Core` 收口），不允许各 mod 自己开。
- 一个宿主可同时服务多个游戏实例，按 `(pid, nonce)` 区分。
- 宿主不在时，客户端必须降级为无桥运行：不抛异常、不阻塞主线程、不往游戏目录之外写东西。

## 2. 发现

宿主在启动时（以及每次拉起游戏前）原子写入：

| 平台 | 路径 |
|---|---|
| Windows 原生 | `%ProgramData%\CasualtiesHub\bridge.json` |
| Linux / Proton | `<prefix>/drive_c/ProgramData/CasualtiesHub/bridge.json` |
| macOS（CrossOver / Whisky） | `<bottle>/drive_c/ProgramData/CasualtiesHub/bridge.json`，bottle 由用户在设置里手动指定 |

```json
{
  "proto": 1,
  "host": { "pid": 1234, "version": "0.1.0", "os": "linux" },
  "endpoint": { "host": "127.0.0.1", "port": 47821 },
  "token": "<64 位小写 hex>",
  "started_at": "2026-11-15T09:00:00Z"
}
```

规则（违反任一条，客户端视为“没有宿主”）：

- 写入用临时文件 + rename，权限 0600；不要原地改写。
- 宿主正常退出时删除；残留文件无害，连接被拒即视为过期。
- 客户端不信任 `endpoint.host` 以外的地址字段；`host` 必须等于 `127.0.0.1`。
- 客户端读不到、解析失败、连不上 → 退避重试（1/2/5/10/30 秒封顶），只在第一次失败时写一条日志。
- 客户端不得因为桥的任何失败影响游戏启动与主循环。

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
| `token` | 发现文件里的 token，常量时间比较 |
| `proto` | `{major, minor}`；`major` 不等即拒 |
| `caps` | 客户端能理解的扩展方法名列表 |
| `game` | `{pid, unity_version, bepinex_version}` |
| `mods` | `[{guid, version}]`，进程内已加载的模组 |

`host → guest: hello.ok`：`res` 的 `result` 为 `{host_version, caps, commands, perms}`（`ok:true`）。

`commands` 是宿主已知的命令表，`perms` 是每条命令当下的授权（`granted` / `denied` / `ask`）。授权不在握手里改，只能由用户在 cuhub 里改，改完用 `evt: perms.changed` 通知。

拒绝：先回 `res.ok:false` 再断开；`bad_token`（message 只写 token rejected，不回详情）、`proto_mismatch`（附带双方 major）、`duplicate_pid`、`busy`（已有会话）。

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
| `busy` | 同一条命令已有一次在飞 |
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

## 7. 权限

| 档 | 默认 | 执行条件 |
|---|---|---|
| `read` | 允许 | 连接即可读 |
| `control` | 关闭 | 用户在设置里显式开启 |
| `mutate` | 关闭 | 每次执行前两段式确认；可选“本次游戏运行内允许” |

- 授权持久化在 `Settings.json` 的 `bridge.permissions`，按 `mod guid` + 命令 id 记。
- 新连接、新命令出现时一律按默认档处理，不继承上次的临时授权。
- 任何授权变更写日志（旧值、新值、触发来源）。

## 8. 安全边界

- 只监听 `127.0.0.1` 的临时端口；不监听 `0.0.0.0`，不做局域网发现。
- 游戏侧发来的一切按不可信输入处理：帧上限、字段长度、id 白名单、数值范围。
- 凭据（Nexus key、GitHub token）永不进入桥；桥上字段以本文件为白名单，新增字段要过一遍这里。
- 宿主不得把桥当作文件系统或进程控制通道：不做任意路径读写、不做进程注入、不做通用 RPC。
- 客户端不得在桥上执行宿主给的路径或代码；handler 只做它自己声明过的事。

## 9. 版本

- `proto.major` 变更 = 破坏性，两端都拒连并给出可读原因。
- `proto.minor` 变更 = 只增字段，旧端忽略。
- 命令表只增不删；删命令等同破坏性变更。
- 第三方 mod 的接入面是 `CuhubBridge.Core` 的公开 API + 本文件，不是 `CuhubBridge.Plugin` 的内部实现。

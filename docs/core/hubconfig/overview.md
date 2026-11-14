# hubconfig

## 职责

按版本选择公告频道（stable / prerelease），用 30 分钟间隔与 ETag / If-Modified-Since 拉取 `HubConfig`，并维护按频道分离的本地缓存。远端链接只保留 https。本模块不碰 UI，也不决定公告如何在界面上展示。

## 输入输出

- `BASE_URL`、`REFRESH_INTERVAL_MINUTES = 30`、`MAX_CONFIG_BYTES = 1 MiB`。
- `HubConfigChannel::{Stable, Prerelease}`；`for_version(&HubVersion)` 按 `is_prerelease()` 选择频道；`as_str()` 给出 `stable` / `prerelease`。
- `HubConfig { current_announcement, previous_announcements, links }`，字段与远端 JSON 的 camelCase 对应，同时接受 PascalCase 别名。
- `HubAnnouncement { id, published_at_utc, message }`；`publishedAtUtc` 按 RFC 3339 解析。
- `HubConfigResult { config, is_online, is_cached, config_changed, next_check_utc }`，语义与上游 `HubConfigResult` 一致。
- `HubConfigService::new(network, app_data, channel)`；`config_url()`、`cache_path()`、`state_path()`、`load_cached()`、`is_check_due()`、`refresh(force)`。
- `refresh` 是同步 API，内部调用 `core::network` 的阻塞客户端；调用方负责把它放到 worker / 阻塞池，UI 不直接调用。

## 缓存与状态

- 缓存文件：`HubConfigCache.{stable|prerelease}.json`；状态文件：`HubConfigHttpState.{stable|prerelease}.json`。两个频道互不读取对方文件。
- 缓存保存远端原始响应体；状态保存 `ETag`、HTTP `Last-Modified` 原值与 `LastCheckedUtc`（RFC 3339）。文件名与 FEATURES §3 一致；上游用枚举名拼文件名，实际生成的是首字母大写，迁移旧缓存时需要改名。
- `load_cached()` 不发起请求，返回缓存与上次检查时间加 30 分钟；没有缓存时返回空 `HubConfig`。
- `is_check_due()`：没有 `LastCheckedUtc` 或距上次检查已满 30 分钟。
- `refresh(false)` 未到期时直接返回缓存；`refresh(true)` 强制请求。
- 304：保留缓存，更新 `LastCheckedUtc`，`is_online = true`、`is_cached = true`、`config_changed = false`。
- 200：解析通过后先写缓存，再更新状态；`is_online = true`、`is_cached = false`、`config_changed = true`。
- 网络失败、超时、非 200、UTF-8 失败或 JSON 解析失败：回退到缓存，`is_online = false`、`is_cached = true`、`config_changed = false`，但 `LastCheckedUtc` 仍更新，避免每次启动都重试。
- 解析失败或无效响应绝不覆盖已有缓存；缓存写入与状态写入都先写 `.json.tmp` 再 rename。

## 远端内容校验

- 只接受绝对 https 链接；带用户名/密码的 URL 直接丢弃。`discordUrl`、`reportUrl` 被丢弃后为空字符串，界面据此隐藏按钮。
- `currentAnnouncement` 缺 `id` 或 `message`（空白也算缺）时，整份配置视为无效，保留旧缓存。
- `previousAnnouncements` 中不可发布的条目逐条过滤，不拖垮整份配置。
- `links`、`previousAnnouncements` 缺失或为 `null` 时按空值处理；文档必须是 JSON 对象，空文本、`null`、数组都拒绝。
- 所有远端字段都按不可信输入处理；本模块只解析与缓存，不执行链接、不打开浏览器。

## 边界与失败模式

- 不内嵌离线公告副本。FEATURES S1 要求内嵌一份，但上游数据是 AGPL，是否搬运待许可决定；当前无缓存时公告为空。
- 不记录日志；`HubConfigError` 只覆盖本地缓存/状态的 IO 与状态序列化错误。网络错误按上游语义吞掉并以 `is_online = false` 表示，调用方自行决定是否记日志或提示。
- 不持有互斥锁。上游用 `SemaphoreSlim` 防并发刷新；本实现假定同一频道只由一个 worker 调用。若以后多个调用方并发，需要在 Hub 层串行化。
- 不做公告已读状态、不决定通知时机；这些属于 app / Hub。

## 常量出处

URL、30 分钟间隔、频道规则、304/200 与失败回退语义来自上游 `Services/HubConfigService.cs` 与 `Models/HubConfig.cs`；数据布局见 `docs/FEATURES.md` §3，公告频道规则见 S3，HTTP 约束见 §4.2。

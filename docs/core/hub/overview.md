# hub

## 职责

`Hub` 是 app 访问 core 的唯一门面。它持有 appdata、设置、日志、目录、社区元数据缓存和网络客户端；app 只读 `Hub` 的快照并调用门面方法。

## 输入输出

- `Hub::open()` 在真实 appdata 根下加载设置、启动会话日志、加载磁盘目录与元数据缓存，并创建网络客户端。
- `set_game_folder` 校验游戏目录后写入 `Settings.json`；`detect_game_folder` 先用 Steam 清单发现安装。
- `plugins_dir` 把游戏根解析到 `BepInEx/Plugins`；`scan_mods` 调用 `mods::scan` 并写一条活动日志。
- `set_mod_enabled` / `set_all_mods_enabled` 走 `mods` 的改名；`delete_scope` / `delete_all_scope` 只返回将删的一级条目与文件数，`delete_mod` / `delete_all_mods` 先确认游戏未运行再执行删除，并写活动日志。
- `save_github_token` / `clear_github_token` 走 `nexus_auth::GithubTokenStore`；`validate_github_token` 请求 `GET https://api.github.com/rate_limit` 并返回剩余额度，401 映射成 `HubError::GithubTokenRejected`。`check_release` 在有 token 时给 release 请求加 Bearer 头，没有 token 时行为不变。
- `launch_game` 调用 `launch::launch_game`，返回 app id 并记录一条活动日志；找不到游戏目录或 app id 时返回错误。
- `metadata_needs_refresh` 在内存列表为空或缓存超过 6 小时时返回 true，调用方据此决定是否后台刷新。
- `refresh_metadata(force)` 通过 `network` 做条件请求，更新缓存与内存列表；`force` 为 true 时忽略 ETag/Last-Modified，供手动刷新使用。
- `open_nexus_download_page` 从已缓存条目取 Nexus Files 页链接，只接受 HTTPS，然后交给平台层打开浏览器。
- `hub_config_cached()` 只读本地 Hub 配置缓存，返回 `HubConfigResult`，不发起网络请求；没有网络客户端时返回空配置与 `is_cached=false`，缓存缺失或无法解析时返回空配置。
- `save_settings` 保存前夹紧 `uiScale`，成功后才替换内存中的设置。

## 边界与失败模式

- 所有阻塞操作由调用方放在 worker 上；app 启动时把过期的目录刷新放到 worker，用户触发的本地扫描仍是同步调用。
- 没有游戏目录时 `plugins_dir` / `scan_mods` / `launch_game` 返回 `HubError::NoGameFolder`。
- 安装与删除共用 `game_running` 防护：游戏运行时返回 `HubError::GameRunning`，文案要求先关闭游戏再改动模组文件。
- 网络失败不会清空缓存；`refresh_metadata` 的错误直接返回给调用方，缓存由 `metadata::MetadataCache` 决定。
- 目录条目缺少 Nexus 链接、链接不是 HTTPS 或无法启动浏览器时，`open_nexus_download_page` 返回对应错误，不尝试降级到不受信任的 URL。
- 设置解析失败已经在 `SettingsStore` 里降级为默认值，`Hub` 只记录警告，不覆盖原文件。
- 日志 IO 错误由 `Logs` 吞掉，不影响核心流程。

## 常量与格式出处

- appdata 根：`platform::app_data_root`，与上游 `HubPaths.AppDataRoot` 对应，目录名为 `CUHub`。
- 元数据 URL、缓存文件和 6 小时生命周期来自 `metadata`。
- 依赖与冲突目录来自 `catalogs`。

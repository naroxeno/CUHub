# hub

## 职责

`Hub` 是 app 访问 core 的唯一门面。它持有 appdata、设置、日志、目录、社区元数据缓存和网络客户端；app 只读 `Hub` 的快照并调用门面方法。

## 输入输出

- `Hub::open()` 在真实 appdata 根下加载设置、启动会话日志、加载磁盘目录与元数据缓存，并创建网络客户端。
- `set_game_folder` 校验游戏目录后写入 `Settings.json`；`detect_game_folder` 先用 Steam 清单发现安装。
- `plugins_dir` 把游戏根解析到 `BepInEx/Plugins`；`scan_mods` 调用 `mods::scan` 并写一条活动日志。
- `launch_game` 调用 `launch::launch_game`，返回 app id 并记录一条活动日志；找不到游戏目录或 app id 时返回错误。
- `metadata_needs_refresh` 在内存列表为空或缓存超过 6 小时时返回 true，调用方据此决定是否后台刷新。
- `refresh_metadata(force)` 通过 `network` 做条件请求，更新缓存与内存列表；`force` 为 true 时忽略 ETag/Last-Modified，供手动刷新使用。
- `open_nexus_download_page` 从已缓存条目取 Nexus Files 页链接，只接受 HTTPS，然后交给平台层打开浏览器。
- `save_settings` 保存前夹紧 `uiScale`，成功后才替换内存中的设置。

## 边界与失败模式

- 所有阻塞操作由调用方放在 worker 上；app 启动时把过期的目录刷新放到 worker，用户触发的本地扫描仍是同步调用。
- 没有游戏目录时 `plugins_dir` / `scan_mods` / `launch_game` 返回 `HubError::NoGameFolder`。
- 网络失败不会清空缓存；`refresh_metadata` 的错误直接返回给调用方，缓存由 `metadata::MetadataCache` 决定。
- 目录条目缺少 Nexus 链接、链接不是 HTTPS 或无法启动浏览器时，`open_nexus_download_page` 返回对应错误，不尝试降级到不受信任的 URL。
- 设置解析失败已经在 `SettingsStore` 里降级为默认值，`Hub` 只记录警告，不覆盖原文件。
- 日志 IO 错误由 `Logs` 吞掉，不影响核心流程。

## 常量与格式出处

- appdata 根：`platform::app_data_root`，与上游 `HubPaths.AppDataRoot` 对应，目录名为 `CUHub`。
- 元数据 URL、缓存文件和 6 小时生命周期来自 `metadata`。
- 依赖与冲突目录来自 `catalogs`。

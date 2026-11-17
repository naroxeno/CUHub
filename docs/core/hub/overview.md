# hub

## 职责

`Hub` 是 app 访问 core 的唯一门面。它持有 appdata、设置、日志、目录、社区元数据缓存和网络客户端；app 只读 `Hub` 的快照并调用门面方法。

## 输入输出

- `Hub::open()` 在真实 appdata 根下加载设置、启动会话日志、加载磁盘目录与元数据缓存，并创建网络客户端。
- `set_game_folder` 校验游戏目录后写入 `Settings.json`；`detect_game_folder` 先用 Steam 清单发现安装。
- `plugins_dir` 把游戏根解析到 `BepInEx/Plugins`；`scan_mods` 调用 `mods::scan` 并写一条活动日志。
- `remote_install_issues(entry)` 是只读检查：扫描已装模组，把清单条目的 `depends` 与依赖目录合并后交给 `mods::dependencies`，返回缺失/过旧列表，不下载。
- `prepare_remote_install(entry)` 在下载前先跑一次依赖检查，有问题直接返回 `HubError::Dependencies`；下载并 stage 后再读 staging 里 DLL 的 `BepInDependency` 复查一遍，第二次不通过会删掉 staging 再返回同一错误。通过后才生成首次安装计划。
- `plan_download` 生成更新计划后用同一套检查复查依赖；包内自带的 DLL（按文件名或插件 GUID 匹配）视为已提供，缺失/过旧同样返回 `HubError::Dependencies` 并删 staging。
- `set_mod_enabled` / `set_all_mods_enabled` 走 `mods` 的改名；`delete_scope` / `delete_all_scope` 只返回将删的一级条目与文件数，`delete_mod` / `delete_all_mods` 先确认游戏未运行再执行删除，并写活动日志。
- `save_github_token` / `clear_github_token` 走 `nexus_auth::GithubTokenStore`；`validate_github_token` 请求 `GET https://api.github.com/rate_limit` 并返回剩余额度，401 映射成 `HubError::GithubTokenRejected`。`check_release` 在有 token 时给 release 请求加 Bearer 头，没有 token 时行为不变。
- `launch_game` 调用 `launch::launch_game`，返回 app id 并记录一条活动日志；找不到游戏目录或 app id 时返回错误。
- `metadata_needs_refresh` 在内存列表为空或缓存超过 6 小时时返回 true，调用方据此决定是否后台刷新。
- `refresh_metadata(force)` 通过 `network` 做条件请求，更新缓存与内存列表；`force` 为 true 时忽略 ETag/Last-Modified，供手动刷新使用。
- `open_nexus_download_page` 从已缓存条目取 Nexus Files 页链接，只接受 HTTPS，然后交给平台层打开浏览器。
- `open_external_link` 是 About 页等固定外链的入口：只接受 HTTPS，交给平台层打开浏览器并记一条活动日志；不是通用下载器，不跟随应用内跳转。
- `hub_config_cached()` 只读本地 Hub 配置缓存，返回 `HubConfigResult`，不发起网络请求；没有网络客户端时返回空配置与 `is_cached=false`，缓存缺失或无法解析时返回空配置。
- `save_settings` 保存前夹紧 `uiScale`，成功后才替换内存中的设置。

## 边界与失败模式

- 所有阻塞操作由调用方放在 worker 上；app 启动时把过期的目录刷新放到 worker，用户触发的本地扫描仍是同步调用。
- 没有游戏目录时 `plugins_dir` / `scan_mods` / `launch_game` 返回 `HubError::NoGameFolder`。
- 安装与删除共用 `game_running` 防护：游戏运行时返回 `HubError::GameRunning`，文案要求先关闭游戏再改动模组文件。
- 依赖门禁在准备阶段就阻断，不浪费下载与 staging；首次安装和更新都走同一套检查。`HubError::Dependencies` 带上每条缺失/过旧的名字、最低版本、已装版本与清单条目（可能为空），调用方据此展示可一键安装项与需手动处理项。
- 网络失败不会清空缓存；`refresh_metadata` 的错误直接返回给调用方，缓存由 `metadata::MetadataCache` 决定。
- 目录条目缺少 Nexus 链接、链接不是 HTTPS 或无法启动浏览器时，`open_nexus_download_page` 返回对应错误，不尝试降级到不受信任的 URL；`open_external_link` 对非 HTTPS 链接返回 `UnsafeLink`。
- 设置解析失败已经在 `SettingsStore` 里降级为默认值，`Hub` 只记录警告，不覆盖原文件。
- 日志 IO 错误由 `Logs` 吞掉，不影响核心流程。

## 常量与格式出处

- appdata 根：`platform::app_data_root`，与上游 `HubPaths.AppDataRoot` 对应，目录名为 `CUHub`。
- 元数据 URL、缓存文件和 6 小时生命周期来自 `metadata`。
- 依赖与冲突目录来自 `catalogs`；远程条目的依赖来自 `sources::SourceEntry.depends`，DLL 声明来自 `mods::dependencies::read_dependencies`。

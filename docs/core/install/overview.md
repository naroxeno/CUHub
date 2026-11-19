# install

## 职责

负责解包到 staging、归档布局与归属规则、BepInEx 本体的检测与安装计划（pin 死的发行版 + 白名单，见下）。普通模组的安装/更新已改由 `packages` 落 Store 并按档案部署；本模块的 `plan_download` / `apply_install` 只保留给 BepInEx 与回退。不决定来源、不碰界面。

## 输入与输出

- `stage_archive(archive, staging_root)`：压缩包路径与一个尚不存在的 staging 目录；返回 `StagedArchive { staging_root, files }`。
- `stage_dll(dll, staging_root)`：release 只发裸 DLL 时使用；校验 `.dll` 扩展与 PE 头（`MZ`）后复制成只有一个文件的 `StagedArchive`。
- `plan_payload(staged, PlanTarget)`：可安装文件只有一个 DLL 时按裸 DLL 规则规划（名字优先；该模组只有一个已装 DLL 时，release 改名也替换它），否则交给 `plan_upgrade`。
- `plan_upgrade(staged, PlanTarget)`：根据归档布局、目标根与当前模组的归属范围生成 `UpgradePlan`；计划里同时列出冲突与警告。
- `plan_first_install(staged, plugins, name)`：旧链路的首次安装计划，规则见「首次安装（旧链路）」；正常远程安装已改走 `packages`。
- `normalize_install_name(name)`：把清单显示名变成可用作目录名的字符串。
- `detect(game_root)`：读 BepInEx 本体的安装状态（是否安装、是否完整、5 或 6、是否 IL2CPP）。
- `plan_install(game_root, staged, Mode)`：按白名单为本体生成计划；Fresh 把既有目标全部记为冲突，Repair 只替换白名单内的普通文件。
- `ensure_plugins(game_root)`：安装完成后确保 `BepInEx/plugins` 存在。
- `apply_upgrade(plan, backup_root)`：只有 `can_apply()` 为真才执行；返回备份目录、替换列表与新增列表。

## 布局

- 归档含 `BepInEx/plugins/...` 时走 BepInEx 布局，只安装 `plugins/` 之后的部分；BepInEx 本体、`core`、`config` 等路径不写。
- 归档只含 DLL/资源、或含 `plugins/...` 时走 Mod 布局；`plugins/` 前缀映射到游戏的 Plugins 根，其余映射到当前模组目录。
- 裸 DLL 不套布局：优先按文件名（不区分大小写）匹配当前模组已装 DLL；找不到但该模组只有一个已装 DLL 时，直接替换它（覆盖 release 把空格改成点这类改名），并在计划里给出 warning。有多个已装 DLL 时仍要求名字唯一匹配，同名多个或找不到都直接失败，不会新建第二个 DLL。
- 去掉所有文件共有的单层 wrapper；`README.txt`、`.DS_Store`、`Thumbs.db`、`desktop.ini`、`__MACOSX` 忽略。

## 归属

已存在的目标文件只有在当前模组的 `source_path`（目录或文件）或 `dll_paths` 内才允许替换。其他已存在文件一律记为 conflict，计划不可执行。新增文件允许，但父链必须是目录或尚不存在；父链遇到 symlink 或普通文件时拒绝。

## 首次安装（旧链路）

远程模组页现在生成 `PackagePlan`。这里的 `plan_first_install(staged, plugins, name)` 只保留给旧链路与测试，复用同一套 `UpgradePlan` 与 `apply_upgrade`；首次安装没有归属信息，规则比更新更严。

- 裸 DLL：落到 `BepInEx/plugins/<DLL 文件名>`，布局记 `Dll`，动作只有 `Create`；文件名沿用 release 资产名，不按清单显示名改名。
- 归档：落到 `BepInEx/plugins/<规范化显示名>`。布局判定与 wrapper 剥离同 `plan_upgrade`；归档带 `plugins/` 前缀或含 `BepInEx/plugins/...` 时目标根会升到 `BepInEx/plugins`，与更新路径一致。
- 规范化名字：只保留 `[A-Za-z0-9._-]`，其余字符换成 `-`，去掉首尾的 `.`/`-`/空格；结果为空时用 `mod`。Windows 保留设备名（`CON`、`PRN`、`AUX`、`NUL`、`COM1–9`、`LPT1–9`，不区分大小写，按第一个点之前的部分判断，因此 `con.dll` 也会被处理）末尾加 `-`。
- 已存在即拒绝：目标 DLL 或目标目录已存在时返回 `PlanError::AlreadyExists`，不合并进已有目录。
- 不覆盖他人文件：首次安装没有 `source_path`，`owned_files` 为空且目标根尚不存在，所以任何已存在的目标都记为 conflict、计划不可执行；只有目标根升到 `BepInEx/plugins` 时才会出现这种冲突。每个目标仍必须解析在 plugins 之内，越界是 `Outside`。

## 限制

- 最多 4096 个条目、单文件 512 MB、解包总量 2 GB。裸 DLL 与归档单文件同受 512 MB 上限，没有前两字节 `MZ` 即失败。
- 归档条目必须是 UTF-8 相对路径；拒绝绝对路径、盘符、`..`、冒号、尾点/尾空格、Windows 保留名、symlink 条目。
- 重复路径按小写比较拒绝，避免大小写不敏感文件系统上的覆盖。

## BepInEx 本体

`bepinex.rs` 负责 BepInEx 本体的检测与安装计划。它和模组安装共用 `UpgradePlan` 与 `apply_upgrade`，但归属规则不同：本体没有所属模组，也没有 `source_path`，白名单就是它的归属规则，普通模组的归属判断在这里不适用。

检测看游戏根：`BepInEx/` 目录、`core/BepInEx.dll`（5）或 `core/BepInEx.Core.dll`（6）、`winhttp.dll` 与 `doorstop_config.ini` 齐备才算完整，有目录但缺文件记为不完整；`major` 与 `version` 从 core DLL 的装配版本读。IL2CPP 用游戏根的 `GameAssembly.dll` 判断——该文件只出现在 IL2CPP 构建里，且放在根目录检查，避免把 6 的安装误判成 IL2CPP。

两条发行版，各自 pin 死版本、文件名、URL 与 sha256，不查询更新：

- BepInEx 5 LTS：5.4.23.6，`BepInEx_win_x64_5.4.23.6.zip`，来自 GitHub Releases；资产根就是 `BepInEx/` 与根目录的 doorstop、说明文件。
- BepInEx 6（Mono，experimental）：6.0.0-be.793，`BepInEx-Unity.Mono-win-x64-6.0.0-be.793+dbc82f0.zip`，来自 BepInEx 的 builds 站。资产布局与 5 同形（`BepInEx/` 加根目录的 doorstop 与说明文件），靠 `core/BepInEx.Core.dll` 与 5 区分。同为 Mono x64，不适用于 IL2CPP 游戏；nightly 通道会重建版本，所以 pin 到具体 build，禁止跟随“最新”。

白名单：`BepInEx/` 下除 `config`、`plugins`、`patchers` 及其子树之外的文件，加上游戏根的 `.doorstop_version`、`doorstop_config.ini`、`changelog.txt`、`winhttp.dll`。三棵子树是用户数据；doorstop 与注入器必须与可执行文件同级；上游压缩包里可能带示例插件或其它工具，白名单把它们挡在游戏目录外，只在计划里报 warning。下载大小与解包限制沿用压缩包的通用约束，sha256 在解包前校验。

两种模式：Fresh 用于未安装，目标已存在一律 conflict；Repair 只替换白名单内的既有普通文件，symlink 与目录仍 conflict，`config`/`plugins` 永远保留。安装完成后确保 `BepInEx/plugins` 存在。备份落在 `<AppData>/Backups`，与模组安装共用一条备份链，界面不承诺备份在游戏目录内。

## 失败模式

- `ArchiveError`：不支持的格式、路径越界、条目过大、加密/多卷/重定向条目、staging 写入失败、裸 DLL 缺 PE 头。
- `PlanError`：没有可安装文件、路径越界、重复路径、首次安装的目标已存在、裸 DLL 不是单文件/不叫 DLL/匹配不到或有多个同名已装 DLL。只有一个已装 DLL 时名字不匹配不算失败，走替换并给 warning。
- `ApplyError`：冲突未解决、目标在排队期间变化、备份或替换失败、回滚失败。回滚失败会同时带上原始错误与问题列表。备份路径必须解析在目标根内，越界时报 `Outside`，不会把绝对路径当备份名。
- 本体安装：`PlanError::Unsupported` 表示游戏是 IL2CPP；sha256 不符或下载失败在解包前终止；白名单过滤后没有文件则 `NoPayload`。

## 接线

普通模组的统一链路：GitHub 自动下载走 `download_github_asset`，Nexus 自动安装走 `download_nexus_latest_cached`，归档都落到 `<AppData>/Cache/Downloads/<stamp>-<pid>` 并保留上游原名；Nexus 页面的手动下载仍落用户的 `download_path`，不参与缓存。`Hub::prepare_package_update` / `prepare_remote_install` 解包或复制到 `<AppData>/Staging/<nanos>-<pid>`，调用 `packages::plan_package` 生成 `PackagePlan`；UI 的 pending 面板展示文件数、目标与警告，用户点 Apply 后 `Hub::apply_package` 入库、写活动档案、按档案部署。取消或计划失败时删 staging。`finish_package` 在生成最终 `files` 前按插件 GUID 去重：包内 DLL 若与已安装的另一个模组同 GUID、且目标不是该模组自身目录，就跳过并在计划警告里说明，避免 CT 这类整包自带的 CUCoreLib 与独立安装重复。

旧的 `plan_download` + `apply_install` 链路只用于 BepInEx 与回退：`plan_download` 解包后生成 `UpgradePlan`，`apply_install` 在游戏未运行时备份并逐文件替换。两者不再处理普通模组的远程安装/更新。

来源选择在 UI 层：pin 优先；未 pin 时有 Nexus Premium 走 Nexus，否则 GitHub；Nexus 下载失败且未 pin 时回退 GitHub，并在面板注明。GitHub 资产按清单 `asset` 精确名选择，未填时取 release 里第一个 zip/7z/rar 或裸 DLL；裸 DLL 下载后同样走 `stage_dll` 与 `plan_payload`。

`apply_install` 先查游戏进程：运行中返回 `GameRunning`，UI 每 3 秒重试，最多 60 次（约 3 分钟）；超时后停止重试并提示游戏仍在运行。取消只在下一次重试前生效，已经开始的写入会完成；不会留下半套文件。写入前把所有 Replace 备份到 `<AppData>/Backups/<UTC 时间戳>`，逐文件临时文件 + rename，失败按逆序回滚。备份目录保留供手工恢复；本版没有恢复界面。

BepInEx 本体复用同一条链路：`Hub::prepare_bepinex` 下载到 `<AppData>/Cache/Downloads/<stamp>-<pid>`、按 pin 的 sha256 校验、解包到 staging 再生成计划；用户确认后 `apply_install`，最后 `finish_bepinex` 建 `BepInEx/plugins`。取消删除 staging，备份同样落在 `<AppData>/Backups`。

远程首次安装走 `Hub::prepare_remote_install(entry)`：先查游戏进程并确认 plugins 存在，按清单的 `repo`/`asset` 下载到 `<AppData>/Cache/Downloads/<stamp>-<pid>`，再 staging 并生成 `PackagePlan`。返回的 `RemoteInstall { name, staging, package }` 交给 pending 面板，确认后 `Hub::apply_package`；下载之后的任何失败都会删掉 staging。`plan_first_install` 仅供旧链路与测试使用。

不做：一键升级、后台自动安装、release 页链接、把归档送进收件箱再确认。Update 在 `OutOfDate`、或生效源为 GitHub 且 release 版本高于已装版本时出现；禁用模组与无法确定来源的行不给，Nexus Standard 的 403 仍按失败展示。

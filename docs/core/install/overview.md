# install

## 职责

把用户选定的压缩包或裸 DLL 变成一次可审查的单模组升级：解包到 staging、按归属规则生成计划、备份后替换、失败回滚。不联网、不决定来源、不碰界面。

## 输入与输出

- `stage_archive(archive, staging_root)`：压缩包路径与一个尚不存在的 staging 目录；返回 `StagedArchive { staging_root, files }`。
- `stage_dll(dll, staging_root)`：release 只发裸 DLL 时使用；校验 `.dll` 扩展与 PE 头（`MZ`）后复制成只有一个文件的 `StagedArchive`。
- `plan_payload(staged, PlanTarget)`：可安装文件只有一个 DLL 时按裸 DLL 规则规划（名字优先；该模组只有一个已装 DLL 时，release 改名也替换它），否则交给 `plan_upgrade`。
- `plan_upgrade(staged, PlanTarget)`：根据归档布局、目标根与当前模组的归属范围生成 `UpgradePlan`；计划里同时列出冲突与警告。
- `apply_upgrade(plan, backup_root)`：只有 `can_apply()` 为真才执行；返回备份目录、替换列表与新增列表。

## 布局

- 归档含 `BepInEx/plugins/...` 时走 BepInEx 布局，只安装 `plugins/` 之后的部分；BepInEx 本体、`core`、`config` 等路径不写。
- 归档只含 DLL/资源、或含 `plugins/...` 时走 Mod 布局；`plugins/` 前缀映射到游戏的 Plugins 根，其余映射到当前模组目录。
- 裸 DLL 不套布局：优先按文件名（不区分大小写）匹配当前模组已装 DLL；找不到但该模组只有一个已装 DLL 时，直接替换它（覆盖 release 把空格改成点这类改名），并在计划里给出 warning。有多个已装 DLL 时仍要求名字唯一匹配，同名多个或找不到都直接失败，不会新建第二个 DLL。
- 去掉所有文件共有的单层 wrapper；`README.txt`、`.DS_Store`、`Thumbs.db`、`desktop.ini`、`__MACOSX` 忽略。

## 归属

已存在的目标文件只有在当前模组的 `source_path`（目录或文件）或 `dll_paths` 内才允许替换。其他已存在文件一律记为 conflict，计划不可执行。新增文件允许，但父链必须是目录或尚不存在；父链遇到 symlink 或普通文件时拒绝。

## 限制

- 最多 4096 个条目、单文件 512 MB、解包总量 2 GB。裸 DLL 与归档单文件同受 512 MB 上限，没有前两字节 `MZ` 即失败。
- 归档条目必须是 UTF-8 相对路径；拒绝绝对路径、盘符、`..`、冒号、尾点/尾空格、Windows 保留名、symlink 条目。
- 重复路径按小写比较拒绝，避免大小写不敏感文件系统上的覆盖。

## 失败模式

- `ArchiveError`：不支持的格式、路径越界、条目过大、加密/多卷/重定向条目、staging 写入失败、裸 DLL 缺 PE 头。
- `PlanError`：没有可安装文件、路径越界、重复路径、裸 DLL 不是单文件/不叫 DLL/匹配不到或有多个同名已装 DLL。只有一个已装 DLL 时名字不匹配不算失败，走替换并给 warning。
- `ApplyError`：冲突未解决、目标在排队期间变化、备份或替换失败、回滚失败。回滚失败会同时带上原始错误与问题列表。

## 接线

`Hub` 把本模块串成一次用户发起的升级：`download_nexus_latest` 或 `download_github_asset` 把归档放到 `<AppData>/Downloads`；`plan_download` 按扩展名解包或复制到 `<AppData>/Staging/<nanos>-<pid>` 并生成计划；UI 的 pending 面板展示文件数、替换/新增、目标根、警告与冲突，用户点 Apply 才 `apply_install`。取消或计划失败时删 staging。

来源选择在 UI 层：pin 优先；未 pin 时有 Nexus Premium 走 Nexus，否则 GitHub；Nexus 下载失败且未 pin 时回退 GitHub，并在面板注明。GitHub 资产按清单 `asset` 精确名选择，未填时取 release 里第一个 zip/7z/rar 或裸 DLL；裸 DLL 下载后同样走 `stage_dll` 与 `plan_payload`。

`apply_install` 先查游戏进程：运行中返回 `GameRunning`，UI 每 3 秒重试，直到退出或用户取消。取消只在下一次重试前生效，已经开始的写入会完成；不会留下半套文件。写入前把所有 Replace 备份到 `<AppData>/Backups/<UTC 时间戳>`，逐文件临时文件 + rename，失败按逆序回滚。备份目录保留供手工恢复；本版没有恢复界面。

不做：一键升级、后台自动安装、release 页链接、把归档送进收件箱再确认。Update 在 `OutOfDate`、或生效源为 GitHub 且 release 版本高于已装版本时出现；禁用模组与无法确定来源的行不给，Nexus Standard 的 403 仍按失败展示。

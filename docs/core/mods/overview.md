# mods

## 职责

扫描 `<game>/BepInEx/Plugins` 下一级条目，把目录或 `.dll` / `.dll.disabled` 文件视为一个模组；用已缓存的社区目录匹配 DLL 名，产出只读的 `InstalledMod` 快照。另提供禁用/启用（把该模组所有 DLL 在 `.dll` 与 `.dll.disabled` 间改名）与删除（移除 Plugins 下一级条目）。不做安装或网络请求。

## 输入输出

- 输入：Plugins 绝对路径、`MetadataMod` 列表、`Catalogs`、忽略依赖名。
- 输出：按名称排序的 `Vec<InstalledMod>`；失败只来自 Plugins 顶层不可读；BepInEx/Plugins 缺失不算错误，此时返回空表（vanilla 游戏根）。`InstalledMod` 额外带 `assembly_version`、`bepinex_version`、插件 `guid` 与 `declared_dependencies`，供界面显示证据、来源解析与安装前检查。
- 扫描器递归查找目录型模组中的 DLL，跳过符号链接，深度上限 16；单个子目录不可读时跳过而不是让整页失败。
- `set_enabled(plugins, item, enabled)` 按 `item.dll_paths` 逐个改名；`set_all_enabled(plugins, enabled)` 递归收集 Plugins 下全部 DLL 后走同一条改名路径；`set_all_enabled_except(plugins, enabled, skip)` 额外跳过 `skip` 下的路径，供 Hub 避开 `Links.json` 与活动档案托管的路径。进入前校验每个 DLL 都在 Plugins 内、是普通文件（不跟随符号链接）、目标不存在，然后才依次 `fs::rename`。失败逆序回滚已改名的文件，返回实际改名的文件数。
- `delete_scope(plugins, item)` / `delete_all_scope(plugins)` 只统计：返回待删的一级条目绝对路径与递归文件数，供界面确认弹窗展示，不落盘；`delete_all_scope_except(plugins, skip)` 统计时排除 `skip`，`delete_all_except` 同样跳过这些路径。
- `delete_mod(plugins, item)` 按来源路径删除单个模组；`delete_all(plugins)` 删除 Plugins 顶层的全部条目（含非模组文件），返回实际删除的文件数。目录用 `remove_dir_all`，文件与符号链接用 `remove_file`，两者都不跟随符号链接。

## 边界与失败模式

- 只看 Plugins 顶层条目，嵌套目录不单独成为模组。
- 目录里一个 DLL 都没有时该条目不出现。
- 没有元数据匹配的模组仍显示：名称取第一个未禁用 DLL 的茎名，状态为 `Unlisted`。匹配前 `DllNames` 与 DLL 茎名都去掉 `.dll`/`.disabled` 再归一化；由活动档案带 `github:` source 的条目优先用清单显示名命中元数据（并要求该元数据的 `DllNames` 仍包含条目的 `match.dll`），避免整包自带多个 DLL 时被更小的候选抢走。
- 对每个模组中的 DLL 读 .NET 元数据（`dotnetdll`）：取 Assembly 版本、`BepInPlugin` 构造参数里的 GUID 与插件版本，以及 `BepInDependency` 的 GUID、最低版本与 `DependencyFlags`。优先用未禁用 DLL；单个 DLL 解析失败只跳过该 DLL，不让整次扫描失败，也不把依赖 DLL 当成插件。读取前先看文件大小，超过 64 MiB 的 DLL 直接跳过（`MAX_DLL_BYTES`）。
- `BepInDependency` 的第二个参数可能是版本字符串，也可能是 `DependencyFlags` 整数；`SoftDependency`（位 2）记为可选依赖，缺省与 `HardDependency` 都算硬依赖。多个 DLL 的声明按 GUID/name 去重后合并进 `declared_dependencies`。
- `installed_version` 优先 `BepInPlugin` 版本，其次 Assembly 版本；`guid` 优先插件 GUID，其次目录里的 `PluginGuid`。
- 版本比较把 1–4 段点分十进制补齐后逐段比较；全 0（如 0.0.0.0）视为占位，不参与比较。两端都可解析时，安装版本更小才是 `OutOfDate`，否则 `UpToDate`；不可解析或只有占位版本时退回文件修改时间：修改时间不早于目录时间才 `UpToDate`，否则 `Unknown`。不再因为依赖 DLL 或没有版本证据而误报更新。
- `.dll.disabled` 只按文件名后缀判断，大小写不敏感。
- 禁用时在文件名末尾追加 `.disabled`；启用时只去掉这个后缀，保留原扩展名大小写。目标已存在时整次操作不落盘，报 `Cannot rename 'X' because 'Y' already exists.`；混合禁用态的模组启用时会把剩余的禁用 DLL 一并启用。改名后调用方需要重新扫描才能看到新的 `disabled` 与状态。
- 删除只接受 Plugins 的直接子项：`strip_prefix` 后必须恰好一个普通组件，否则报 `OutsidePlugins`；Plugins 自身与嵌套路径一律拒绝。`delete_all` 按 FEATURES E8 删空 Plugins 顶层，非模组文件也会进入确认列表并被删除；`delete_mod` 只动单个来源路径。
- 删除前逐项 `symlink_metadata`：目录删除时若条目其实是符号链接，只删链接本身；不会沿链接删到 Plugins 外。
- 由活动档案部署的条目不走本模块的启用/禁用与单项删除：Hub 改档案条目的 `enabled` 或直接移除条目后重新应用档案，界面显示「档案内」但按钮仍可用。只剩 `Links.json` 记录、活动档案里找不到对应条目的路径，Hub 仍返回 `HubError::ManagedByProfile`。
- 删除不可回滚；批量删除中途失败时前面的条目已消失，调用方应重新扫描而不是重试整批。
- 依赖与冲突在扫描完成后统一计算；被禁用的模组不报告缺失依赖或冲突，被禁用的依赖也不算已满足。

## 依赖检查

- 依赖来源分三类：DLL 里的 `BepInDependency`（随扫描得到，写进 `declared_dependencies`）、`DependencyCatalog.json`（按模组名索引）、远程清单条目的 `depends`（只对这次要安装的条目生效）。
- `dependencies::check_requirements` 只对启用中的已装模组匹配：optional 直接跳过；没有匹配项记 `Missing`；有匹配项但版本低于 `minimumVersion` 记 `Outdated` 并带上已装版本。版本比较复用 `HubVersion`：两段与三段走 `parse_external`，四段先截成三段再比，解析失败不阻断。
- 找不到来源时 `entry` 为空，界面只能给 Nexus 搜索；清单能匹配到条目时带上 `SourceEntry`，界面可以一键安装。已知依赖名仍可经 `ignored_dependency_names` 忽略，忽略只作用于行内标注，不解除安装门禁。
- `missing_labels` 把检查结果格式化成 `名称 最低版本+` 供 `annotate_dependencies` 写进 `InstalledMod.missing_dependencies`；安装前门禁在 `Hub::prepare_remote_install` 与 `Hub::plan_download`，本模块不下载、不写盘。
- 安装前检查看的是完整 `InstalledMod` 表，包括刚下载但尚未 Apply 的 staging DLL（按 `read_dependencies` 读取），因此补丁包里新带的依赖也会被拦下。staging 内 DLL 自身的 identity（文件名 + `BepInPlugin` GUID）同时被当作包内已提供项，避免把随包附带的依赖误判为缺失。

## 常量与格式出处

- Plugins 位置：上游 `Services/ModService.cs` 的 `GetPluginsPath`，固定为 `BepInEx/Plugins`；路径大小写由 `paths::find_child` 还原。
- 条目识别与排序：`Services/ModService.cs` 的 `GetInstalledMods`。
- 元数据匹配优先级：`Services/ModService.cs` 的 `FindBestMetadataMatch`；FEATURES E17。去扩展名与清单显示名优先是本仓库的扩展，用于整包自带依赖 DLL 的场景。
- 状态文案与缺失依赖、冲突语义：FEATURES E4、E5、E15、E17。
- 禁用/启用与改名冲突文案：FEATURES E6；批量开关与档案条目来自 E8；档案部署、`enabled=false` 与 `Links.json` 的归属见 `docs/core/profiles/overview.md` 与 FEATURES Z。
- 删除范围与破坏性确认文案：FEATURES E8（Delete all）、E9（单项删除）。

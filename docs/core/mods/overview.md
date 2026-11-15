# mods

## 职责

扫描 `<game>/BepInEx/Plugins` 下一级条目，把目录或 `.dll` / `.dll.disabled` 文件视为一个模组；用已缓存的社区目录匹配 DLL 名，产出只读的 `InstalledMod` 快照。另提供禁用/启用：把该模组所有 DLL 在 `.dll` 与 `.dll.disabled` 间改名。不做安装、删除或网络请求。

## 输入输出

- 输入：Plugins 绝对路径、`MetadataMod` 列表、`Catalogs`、忽略依赖名。
- 输出：按名称排序的 `Vec<InstalledMod>`；失败只来自 Plugins 顶层不可读或 BepInEx/Plugins 缺失。`InstalledMod` 额外带 `assembly_version`、`bepinex_version` 与插件 `guid`，供界面显示证据与后续来源解析。
- 扫描器递归查找目录型模组中的 DLL，跳过符号链接，深度上限 16；单个子目录不可读时跳过而不是让整页失败。
- `set_enabled(plugins, item, enabled)` 按 `item.dll_paths` 逐个改名；`set_all_enabled(plugins, enabled)` 递归收集 Plugins 下全部 DLL 后走同一条改名路径。进入前校验每个 DLL 都在 Plugins 内、是普通文件（不跟随符号链接）、目标不存在，然后才依次 `fs::rename`。失败逆序回滚已改名的文件，返回实际改名的文件数。

## 边界与失败模式

- 只看 Plugins 顶层条目，嵌套目录不单独成为模组。
- 目录里一个 DLL 都没有时该条目不出现。
- 没有元数据匹配的模组仍显示：名称取第一个未禁用 DLL 的茎名，状态为 `Unlisted`。
- 对每个模组中的 DLL 读 .NET 元数据（`dotnetdll`）：取 Assembly 版本、`BepInPlugin` 构造参数里的 GUID 与插件版本。优先用未禁用 DLL；单个 DLL 解析失败只跳过该 DLL，不让整次扫描失败，也不把依赖 DLL 当成插件。
- `installed_version` 优先 `BepInPlugin` 版本，其次 Assembly 版本；`guid` 优先插件 GUID，其次目录里的 `PluginGuid`。
- 版本比较把 1–4 段点分十进制补齐后逐段比较；全 0（如 0.0.0.0）视为占位，不参与比较。两端都可解析时，安装版本更小才是 `OutOfDate`，否则 `UpToDate`；不可解析或只有占位版本时退回文件修改时间：修改时间不早于目录时间才 `UpToDate`，否则 `Unknown`。不再因为依赖 DLL 或没有版本证据而误报更新。
- `.dll.disabled` 只按文件名后缀判断，大小写不敏感。
- 禁用时在文件名末尾追加 `.disabled`；启用时只去掉这个后缀，保留原扩展名大小写。目标已存在时整次操作不落盘，报 `Cannot rename 'X' because 'Y' already exists.`；混合禁用态的模组启用时会把剩余的禁用 DLL 一并启用。改名后调用方需要重新扫描才能看到新的 `disabled` 与状态。
- 依赖与冲突在扫描完成后统一计算；被禁用的模组不报告缺失依赖或冲突。

## 常量与格式出处

- Plugins 位置：上游 `Services/ModService.cs` 的 `GetPluginsPath`，固定为 `BepInEx/Plugins`；路径大小写由 `paths::find_child` 还原。
- 条目识别与排序：`Services/ModService.cs` 的 `GetInstalledMods`。
- 元数据匹配优先级：`Services/ModService.cs` 的 `FindBestMetadataMatch`；FEATURES E17。
- 状态文案与缺失依赖、冲突语义：FEATURES E4、E5、E15、E17。
- 禁用/启用与改名冲突文案：FEATURES E6；批量开关来自 E8。

# mods

## 职责

扫描 `<game>/BepInEx/Plugins` 下一级条目，把目录或 `.dll` / `.dll.disabled` 文件视为一个模组；用已缓存的社区目录匹配 DLL 名，产出只读的 `InstalledMod` 快照。不做安装、删除、禁用/启用或网络请求。

## 输入输出

- 输入：Plugins 绝对路径、`MetadataMod` 列表、`Catalogs`、忽略依赖名。
- 输出：按名称排序的 `Vec<InstalledMod>`；失败只来自 Plugins 顶层不可读或 BepInEx/Plugins 缺失。
- 扫描器递归查找目录型模组中的 DLL，跳过符号链接，深度上限 16；单个子目录不可读时跳过而不是让整页失败。

## 边界与失败模式

- 只看 Plugins 顶层条目，嵌套目录不单独成为模组。
- 目录里一个 DLL 都没有时该条目不出现。
- 没有元数据匹配的模组仍显示：名称取第一个未禁用 DLL 的茎名，状态为 `Unlisted`。
- 不读取 Assembly 版本或 BepInEx 插件特性，因此 `installed_version` 目前为空；更新判定只用文件修改时间和目录里的 `LatestFileModifiedUtc`。没有可比时间时状态为 `Unknown`。
- 由于没有已安装版本信号，本模块不判定 `OutOfDate`：只有修改时间不早于目录时间才标记 `UpToDate`，其余保留 `Unknown`，避免把依赖 DLL 或安装时间误报成“有更新”。接入 .NET 元数据读取后再启用版本比较。
- `.dll.disabled` 只按文件名后缀判断，大小写不敏感。
- 依赖与冲突在扫描完成后统一计算；被禁用的模组不报告缺失依赖或冲突。

## 常量与格式出处

- Plugins 位置：上游 `Services/ModService.cs` 的 `GetPluginsPath`，固定为 `BepInEx/Plugins`；路径大小写由 `paths::find_child` 还原。
- 条目识别与排序：`Services/ModService.cs` 的 `GetInstalledMods`。
- 元数据匹配优先级：`Services/ModService.cs` 的 `FindBestMetadataMatch`；FEATURES E17。
- 状态文案与缺失依赖、冲突语义：FEATURES E4、E5、E15、E17。

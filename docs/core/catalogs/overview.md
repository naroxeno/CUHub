# catalogs

## 职责

解析依赖目录 DependencyCatalog.json 与不兼容目录 IncompatibilityCatalog.json，并把可读副本放在应用目录旁供用户编辑。不做网络请求，不修改文件。

## 输入输出

- Catalogs::load(app_dir)：先找 app_dir/Data/Catalogs/，再退回旧路径 app_dir/Services/；第一个存在的文件生效。缺失视为空目录，不是错误；解析失败记录在 errors 并保持空目录。
- DependencyCatalog::load(path)：解析 {dependencies:[{mod, requires:[{name, guid, minimumVersion, note, optional}]}]}；name/guid 至少一个非空，optional 缺省为 false。
- requirements_for(names)：合并多个模组的依赖，按 name+minimumVersion 去重，按名称忽略大小写排序；返回带 display_label 的条目。
- DependencyRequirement::matches(name, guid)：有 guid 且与已装模组 guid 命中时优先按 guid，否则按 name 归一化比较；安装前检查与扫描标注共用这一条规则。
- has_entry_for_any(names)、names_match(left, right)：键名归一化后比较。
- IncompatibilityCatalog::conflicts(mod, installed)：双向匹配 modA/modB，返回对方名称并去重（忽略大小写）。

## 边界与失败模式

- 键名归一化：只保留字母与数字并转小写（与上游 Key 一致）；因此标点和空格不影响匹配。
- 空 mod 名、空依赖名、空 modA/modB 被丢弃；同名模组的多个条目合并。
- optional 只影响安装前检查（跳过），不改变已装模组的 Missing 标注；需要强制安装的依赖不要写 optional。
- 首个存在的文件优先；该文件损坏时不会回退到旧路径，只记录错误。调用方应把 errors 写进日志。
- 当前不内嵌目录数据：内嵌资源（Data/Catalogs 之后、Services 之前）待仓库许可与数据来源确认后补；也不做“文件变了才重载”的缓存，调用方在启动时 load 一次并复用。
- display_label 与上游一致：名称 + 空格 + 最低版本 + "+" + " (" + 备注 + ")"。

## 常量出处

JSON 结构见 docs/FEATURES.md G1/G2/G5；匹配与缓存语义见上游 Services/DependencyCatalogService.cs 与 IncompatibilityCatalog.cs。

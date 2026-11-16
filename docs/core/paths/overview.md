# paths

## 职责

路径发现、真实大小写还原与越界判断。发现不区分大小写（Windows 语义在所有平台一致），身份比较在 Windows 忽略大小写、其他平台按字节序。

## 输入输出

- find_child(parent, name)：返回磁盘上的真实条目路径，找不到返回 None。
- resolve_child：找不到时按请求的大小写拼接，供后续创建。
- is_inside(candidate, container)：先把候选的目录链解析到真实路径，再与容器的真实路径做组件级前缀比较，相等视为在内。叶子本身保留原样：指向别处的符号链接不会被解析成目标路径，缺失的尾部组件按字面补上。
- game_paths(selected)：接受游戏根、BepInEx、BepInEx/Plugins 三种粒度，返回游戏根路径。BepInEx 与 Plugins 的位置固定为 game_root/BepInEx/Plugins，不再单独返回。只要求选中的目录存在且是目录，不要求 BepInEx 已安装——vanilla 游戏根也能选中。

## 边界与失败模式

- 只读文件系统，不创建、不删除、不缓存结果。
- 目录链中的符号链接会被解析，叶子不会被跟随：链接父目录越界能被挡住，但写入前调用方仍要确认目标是普通文件而不是链接。
- 非 UTF-8 文件名按 lossy 字符串比较，可能误判；配置路径均为 UTF-8。
- game_paths 对缺失或非目录的选中路径返回 PathError::Missing，错误带完整路径；BepInEx/Plugins 缺失不再算错。

## 常量出处

BepInEx、Plugins 目录名与三种粒度见 docs/FEATURES.md B1；越界规则见 B4 与上游 Services/HubPaths.cs。

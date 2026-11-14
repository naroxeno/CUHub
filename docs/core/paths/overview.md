# paths

## 职责

路径发现、真实大小写还原与越界判断。发现不区分大小写（Windows 语义在所有平台一致），身份比较在 Windows 忽略大小写、其他平台按字节序。

## 输入输出

- find_child(parent, name)：返回磁盘上的真实条目路径，找不到返回 None。
- resolve_child：找不到时按请求的大小写拼接，供后续创建。
- is_inside(candidate, container)：先做词法规范化（解析 . 与 ..，不解析符号链接），再做组件级前缀比较；相等视为在内。
- game_paths(selected)：接受游戏根、BepInEx、BepInEx/Plugins 三种粒度，返回 GamePaths { game_root, plugins_dir }。

## 边界与失败模式

- 只读文件系统，不创建、不删除、不缓存结果。
- 不解析符号链接，与上游 HubPaths.cs 的 GetFullPath 语义一致；调用方在写入前仍需自己确认目标。
- 非 UTF-8 文件名按 lossy 字符串比较，可能误判；配置路径均为 UTF-8。
- game_paths 对缺失目录返回 PathError::Missing，错误带完整路径。

## 常量出处

BepInEx、Plugins 目录名与三种粒度见 docs/FEATURES.md B1；越界规则见 B4 与上游 Services/HubPaths.cs。

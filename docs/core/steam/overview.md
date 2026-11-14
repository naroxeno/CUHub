# steam

## 职责

从 Steam 自己记录的清单发现库与游戏，不靠猜目录名：解析 libraryfolders.vdf 与 appmanifest_*.acf，在 steamapps/common 下还原真实大小写。只读。

## 输入输出

- VdfNode::parse(text)：KeyValues tokeniser。支持现代 libraryfolders（数字键下 path 子项）与旧式扁平布局（数字键直接存路径）、C 转义、行注释；嵌套深度上限 32。
- candidate_roots()：按平台返回候选 Steam 根。
- find_libraries()：候选根存在时解析 steamapps/libraryfolders.vdf 与 config/libraryfolders.vdf，合并去重；符号链接解析后去重，避免 ~/.steam/steam 重复扫描。
- find_installed_games()：每个库读 appmanifest_*.acf 的 appid/installdir，在 steamapps/common 下按真实大小写解析目录。
- find_casualties_unknown()：installdir 含 casualties（忽略大小写）的第一项，附带 app id。
- find_game_install()：先按清单找，再扫各库 steamapps/common 的目录名；返回游戏根路径。
- app_id_for_game(game_root)：先比对已发现安装的路径；找不到时从游戏根向上找 steamapps，直接读该库的 appmanifest 匹配 installdir。只返回 app id，不启动游戏。

## 边界与失败模式

- 读不了的文件或目录直接跳过；畸形 VDF 退化为空结果，不 panic。
- vdf 中的路径不存在时丢弃；不做路径规范化以外的校验。
- Windows 候选根目前只有 Program Files 环境变量；注册表 SteamPath/InstallPath 未接入，需要新增 winreg 依赖，待决定。
- macOS 没有单独候选根，当前按 Linux 列表处理。

## 常量出处

候选根顺序、两处 libraryfolders 位置、appmanifest 命名与 installdir 判定见上游 Services/SteamLibraryLocator.cs；游戏判定见 docs/FEATURES.md。

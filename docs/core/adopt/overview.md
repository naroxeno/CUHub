# adopt 模块

## 职责

把已经躺在 Plugins 里的旧模组收编进 Store，并原地替换成链接。两段式：`plan_adopt` 只读产出清单，`apply_adopt` 才动文件。

## 清单规则

- 跳过：已停用、没有版本号、已由本工具托管、不是 Plugins 直系子项、mod id 重名、Store 里已有同版本。
- mod id：目录型取目录名，单 DLL 取文件名去扩展名；统一小写，非法字符转 `-`。版本优先 `installedVersion`，其次 `assemblyVersion`。
- 目标路径就是原位置，所以迁移对游戏目录无新增层级。

## 执行顺序

先 import 进 Store（失败即停，原文件不动）→ 把原文件移到 `Backups/Adopt-<时间戳>/` → `LinkIndex::deploy` → 写回 `Links.json`。部分失败用 `AdoptError::Adopt { report, source }` 返回，`adopted` 与 `remaining` 合起来等于输入项；已移走的原文件在备份目录里，不会丢。

## 边界

不处理 config 收编、不做迁移界面、不自动执行；单文件模组依赖 `Store::import_file`，目录模组走 `import_dir`。

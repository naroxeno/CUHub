# adopt 模块

## 职责

把已经躺在 Plugins 里的旧模组收编进 Store，并原地替换成链接。两段式：`plan_adopt` 只读产出清单，`apply_adopt` 才动文件。

## 清单规则

- 跳过：已停用、没有版本号、已由本工具托管（扫描到的路径已有链接、目标路径已有链接，或 `Links.json` 里已有同 mod id）、由 CUHub 安装的桥（`Plugins/CuhubBridge`）、不是 Plugins 直系子项、mod id 重名、目标路径与同批其它项冲突、Store 里已有同版本。
- mod id：目录型取目录名，单 DLL 取文件名去扩展名；统一小写，非法字符转 `-`。版本优先 `installedVersion`，其次 `assemblyVersion`。
- 目标路径与档案部署同一规则：目录型按 Store manifest 计算——恰好一个单段 `.dll` 时目标为 `Plugins/<文件名>`，否则为 `Plugins/<mod_id>`（目录名被规范化，如 `Better Mod` → `better-mod`）；单文件模组保留原文件名。这样迁移后的链接目标与 `deployment_target` 一致，切换档案时不会重定位。

## 执行顺序

先 import 进 Store（失败即停，原文件不动）→ 把原文件移到 `Backups/Adopt-<时间戳>/` → `LinkIndex::deploy` → 写回 `Links.json`。部分失败用 `AdoptError::Adopt { report, source }` 返回，`adopted` 与 `remaining` 合起来等于输入项；已移走的原文件在备份目录里，不会丢。

## 与档案的关系

`Hub::apply_adopt` 在至少一项迁移成功后（含部分完成）调用 `profiles::checkpoint` 新建 `adopt_<时间戳>` 档案并设为激活，档案规则见 `docs/core/profiles/overview.md`。

## 边界

不处理 config 收编、不做迁移界面、不自动执行；单文件模组依赖 `Store::import_file`，目录模组走 `import_dir`。

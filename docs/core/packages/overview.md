# packages

## 职责

把一份已经 staging 好的单模组载荷变成一次完整安装：复制进 Store、写入当前档案、按档案部署到游戏目录，并在替换未托管文件前备份。它不下载、不解析归档布局、不决定来源；这些由 `install` 与 `Hub` 完成。

## 输入与输出

- `PackagePlan { mod_id, version, source, target, previous, staging, files, single, warnings }`：`Hub` 根据 staging 载荷生成。`target` 是 Plugins 下的部署目标，`previous` 只在替换未托管条目时存在。
- `PackageContext { store, links, profiles, game_root, plugins_root, backup_root }`：一次 apply 所需的全部外部状态。
- `PackageReport { stored, files, backup, profile, applied }`：入库版本、文件数、备份目录、写入的档案名与实际部署操作。
- `PackageError`：Store、Profile、Link、载荷路径、档案读写、同版本载荷冲突（`PayloadConflict`）与 IO 错误；部分完成由 `ProfileError::Apply` 透传。

## 流程

1. `import_package` 把 `plan.files` 复制到 `<staging>/package-payload`，再交给 `Store::import_dir`；已有同版本时只在清单与 sha256 全部一致时复用，内容不同则返回 `PayloadConflict`，绝不把旧载荷当成新下载部署。
2. `active_or_default` 取活动档案；没有活动档案时用 `Default`，连 `Default` 都没有时用 `Links.json` 生成一份临时清单。
3. 档案标记为 active，删掉同 `modId` 的旧条目，再追加新版本；随后用 `plan_apply` 生成 `Deploy` / `Remove` 列表。
4. 若 `previous` 存在且目标确实要部署，先把旧路径整体移到 `backup_root/Package-<时间戳>[-n]`；移动失败则回退到复制后删除。
5. `apply_plan` 逐条解链/建链并保存 `Links.json`；只有部署成功后才保存档案。失败时已完成的链接保留，档案不写入新版本，界面可重试。

## 部署形状

- 单 DLL：目标为 `Plugins/<文件名>`，Store 里只有一个根文件。
- 目录：目标为 `Plugins/<mod_id>`，Store 保留整个载荷目录；`mod_id` 来自清单名或旧路径的规范化结果。
- 归档允许带 `BepInEx/plugins/...` 或 `plugins/...` 前缀：映射时只保留 Plugins 之后的路径，其余文件忽略；整包只有插件文件时按单 DLL 或目录形状处理。
- 档案只记录 `modId` / `version`，目标路径由 Store 的 manifest 推导，不写死在档案文件里。

## 边界与失败模式

- 不递归删除游戏目录：替换前只移动 `plan.previous` 这一个已知路径，其余删除一律走 `Links.json`。
- 不处理游戏进程检查；`Hub::apply_package` 在调用前执行 `ensure_game_closed`。
- 不联网、不读归档、不校验 PE 头；载荷内容由 `install` 的 staging 负责。
- Store 写入是不可变的：同版本且内容一致时复用，内容不同直接拒绝；不同版本永远新增目录。
- 档案保存失败不回滚已经完成的链接，也不把新版本写入档案；用户可重新 apply 或切换档案。
- `InvalidRelative` 拒绝绝对路径、`..` 与空路径；`MissingPayload` 表示 staging 或 Store 载荷在 apply 前消失。

## 测试

`packages/tests.rs` 覆盖：单 DLL 入 Store 并落到 Plugins、替换未托管目录先备份、同版本重复 apply 复用 Store、托管条目更新替换档案、同版本不同内容被拒绝。

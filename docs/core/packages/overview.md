# packages

## 职责

把一份已经 staging 好的单模组载荷入库，部署后保存活动档案，并在替换未托管文件前备份。下载、归档解析和来源选择由 `install` 与 `Hub` 完成。

## 输入与输出

- `PackagePlan` 由 Hub 根据 GUID 去重后的载荷生成。最终目标与档案按清单计算的目标使用同一条规则。计划同时记录已确认的旧目标，区分需要备份的散装条目和可按账本解链的托管条目。
- `PackageContext { store, links, profiles, game_root, plugins_root, backup_root }`：一次 apply 所需的全部外部状态。
- `PackageReport { stored, files, backup, profile, applied }`：入库版本、文件数、备份目录、写入的档案名与实际部署操作。
- `PackageError`：Store、Profile、Link、载荷路径、档案读写、同版本载荷冲突（`PayloadConflict`）与 IO 错误；部分完成由 `ProfileError::Apply` 透传。

## 流程

1. `import_package` 把计划载荷交给 Store。复用同版本时检查新包与清单是否匹配，再检查库内实际文件与清单是否匹配，分别定位不同内容和库内变更。
2. `active_or_default` 取活动档案；没有活动档案时用 `Default`，连 `Default` 都没有时用 `Links.json` 生成一份临时清单。
3. 档案标记为 active，删掉同 `modId` 的旧条目，再追加新版本；随后用 `plan_apply` 生成 `Deploy` / `Remove` 列表。
4. 整份差异必须落在包的已确认范围内。未托管目标冲突在移动旧件前报告。已有散装旧件先移到备份目录，跨卷时复制完成后再删除原件。
5. 先完成新目标部署，再处理不同路径的旧托管目标，保存账本后提交档案。备份了散装旧件但新目标部署失败时，尝试把旧件移回空闲的原路径；错误消息带备份位置与可能的恢复错误。

## 来源切换

目录更新复用已安装条目，包括活动档案里已禁用的版本，以保持原有 Store 模组 ID。目录入口可以用已知 BepInEx GUID 验证新载荷，验证失败会丢弃 staging。跨来源同版本仍遵循载荷冲突规则。

## 部署形状

- 单 DLL：目标为 `Plugins/<文件名>`，Store 里只有一个根文件。
- 目录：目标为 `Plugins/<mod_id>`，Store 保留整个载荷目录；`mod_id` 来自清单名或旧路径的规范化结果。
- 归档允许带 `BepInEx/plugins/...` 或 `plugins/...` 前缀：映射时只保留 Plugins 之后的路径，其余文件忽略；整包只有插件文件时按单 DLL 或目录形状处理。
- 档案只记录 `modId` / `version`，目标路径由 Store 的 manifest 推导，不写死在档案文件里。

## 边界与失败模式

- 只有已确认的散装旧件可以整体移动。跨卷回退会先保留完整备份，再清理该旧件。托管条目走部署账本。
- 不处理游戏进程检查；`Hub::apply_package` 在调用前执行 `ensure_game_closed`。
- 不联网、不读归档、不校验 PE 头；载荷内容由 `install` 的 staging 负责。
- Store 写入是不可变的：同版本且内容一致时复用，内容不同直接拒绝；不同版本永远新增目录。
- 部署完成后的档案保存失败单独报告；同一进程可重试收尾。进程强制退出和系统断电的自动恢复未纳入本流程。
- `InvalidRelative` 拒绝绝对路径、`..` 与空路径；`MissingPayload` 表示 staging 或 Store 载荷在 apply 前消失。

## 测试

`packages/tests.rs` 覆盖：单 DLL 入 Store 并落到 Plugins、替换未托管目录先备份、同版本重复 apply 复用 Store、托管条目更新替换档案、同版本不同内容被拒绝。

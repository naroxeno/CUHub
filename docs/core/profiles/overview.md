# profiles 模块

## 职责

一个 profile 是一份「哪些 Store 版本该出现在 Plugins 里」的清单。模块负责：读写 `Profiles/<name>.json`、记录当前激活的 profile、把清单与 `Links.json` 现状做差得到 `ApplyOp`，再执行并写回索引。

## 结构

- `profiles.rs`：模型、`ProfileStore`（list/load/save/remove/active/set_active）、`plan_apply`/`apply_plan`、错误类型。
- `profiles/tests.rs`：临时目录上的真实文件测试，无 mock。

## 关键决定

- `Profiles/active.txt` 是「当前激活」的唯一权威，`Profile::active` 只是镜像；写入时两边同步。
- 部署单元看 Store 的 manifest：恰好一个单段 `.dll` → 文件单元（`Plugins/<文件名>`），否则目录单元（`Plugins/<mod_id>`）。source 在 apply 时从 Store 重算，`ApplyOp` 只带目标。
- `plan_apply` 只处理 `Links.json` 里位于 Plugins 下一层的条目；BepInEx/config 等部署不受 profile 切换影响。
- 目标路径统一由 `deployment_target` 计算（单 DLL 用文件名，否则 `Plugins/<mod_id>`）；`plan_apply`、Hub 的托管判定与禁用条目补全共用它，避免规则漂移。
- `enabled=false` 的条目不部署，但留在档案里；Hub 把这类条目补进本地列表（`source_path` 为目标路径，`disabled=true`），因此禁用后行仍保留、可再启用。
- 部分失败用 `ProfileError::Apply { report, source }` 返回：已成功的链接先写回 `Links.json`，`report.applied` 与 `report.remaining` 加起来等于输入 op 数，界面据此列出待处理项。

## 边界

configOverrides 只持久化不执行；`ProfileEntry::link`/`source` 是元数据，链接种类仍由 `LinkIndex::deploy` 决定。两者都属于后续阶段（配置部署、按 profile 强制链接方式）。

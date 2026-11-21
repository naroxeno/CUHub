# profiles 模块

## 职责

一个 profile 是一份「哪些 Store 版本该出现在 Plugins 里」的清单。模块负责：读写 `Profiles/<name>.json`、记录当前激活的 profile、把清单与 `Links.json` 现状做差得到 `ApplyOp`，再执行并写回索引。

## 结构

- `profiles.rs`：模型、`ProfileStore`（list/load/save/remove/active/set_active/rename）、`plan_apply`/`apply_plan`、错误类型。
- `profiles/tests.rs`：临时目录上的真实文件测试，无 mock。

## 关键决定

- `Profiles/active.txt` 是「当前激活」的唯一权威，`Profile::active` 只是镜像；保存激活档案与切换激活时，会把所有档案文件的 `active` 标记重写成一致状态，保证全库最多一个 `true`。
- 部署单元看 Store 的 manifest：恰好一个单段 `.dll` → 文件单元（`Plugins/<文件名>`），否则目录单元（`Plugins/<mod_id>`）。source 在 apply 时从 Store 重算，`ApplyOp` 只带目标。
- `plan_apply` 只处理 `Links.json` 里位于 Plugins 下一层的条目；BepInEx/config 等部署不受 profile 切换影响。
- 目标路径统一由 `deployment_target` 计算（单 DLL 用文件名，否则 `Plugins/<mod_id>`）；`plan_apply`、Hub 的托管判定与禁用条目补全共用它，避免规则漂移。
- `checkpoint` 供 Hub 迁移收尾调用：以当前激活档案为底，同 mod id 的条目被迁移结果替换，再补齐 `Links.json` 里尚未覆盖的条目，生成 active 档案；名称由 Hub 给（`adopt_%Y%m%d-%H%M%S`，重名加 `-序号`）。
- `rename` 先按新名保存再删旧文件：激活档案改名时 `active.txt` 已由 `save` 指向新名，删旧文件不会清空激活；目标重名返回 `AlreadyExists`，旧档案原样保留。
- `enabled=false` 的条目不部署，但留在档案里；Hub 把这类条目补进本地列表（`source_path` 为目标路径，`disabled=true`），因此禁用后行仍保留、可再启用。
- 部分失败用 `ProfileError::Apply { report, source }` 返回：已成功的链接先写回 `Links.json`，`report.applied` 与 `report.remaining` 加起来等于输入 op 数，界面展示已完成操作数、总数和失败原因。索引保存失败也保留操作报告。

- 档案应用入口在预览与执行前检查部署目标。未托管目标已存在或无法检查时，在修改 Plugins 之前报出目标路径。

## 边界

configOverrides 只持久化不执行；`ProfileEntry::link`/`source` 是元数据，链接种类仍由 `LinkIndex::deploy` 决定。`source` 用 `github:owner/repo` 或 `nexus:<id>` 记录来源，Hub 只按 `github:` 解析身份；`link` 记录部署时的链接种类，供展示与后续阶段使用。

# 交接说明

截至 2026-11-14：**只有设计与文档，没有代码**。`crates/` 尚未创建，`nsh` 工程未初始化。

## 1. 仓库现状

- jj 仓库（colocated，`main` 分支），四个提交：规则与功能清单 → 像素风重写 → 架构文档 → 来源表设计。
- 工作副本 `@` 是空 change，可以直接开工。
- 上游源码快照在 `ref/upstream/`（已进 `.gitignore`，不提交、不发版）。

## 2. 已有文档

| 文件 | 内容 |
|---|---|
| `AGENTS.md` | 代理规则：决策权、先问再做、注释政策、文档渐进披露、架构文档唯一位置 |
| `docs/FEATURES.md` | 全量功能清单（A–U），含像素风外观系统、来源表、里程碑与风险 |
| `docs/ARCHITECTURE.md` | 108 行架构：两 crate、Hub 门面、事件、线程、外部边界、持久化、未决项 |

## 3. 已拍板的决定

| 项 | 决定 |
|---|---|
| 结构 | 两个 crate：`core`（无 UI）+ `app`（nsh），依赖单向 |
| 交互形态 | 方案 C：`Hub` 门面 + 最多 5 个事件变体，不用事件总线 |
| 平台差异 | `core/src/platform/{windows,linux}.rs`，同名函数导出 |
| 外观 | 像素风固定调色板（14 个 token），无用户自定义主题；1x/2x/3x 整数缩放，默认 2x |
| 字体 | Fusion Pixel 12px（OFL），缺字回落 Ark Pixel，再回落等宽 |
| 无障碍 | 保留高对比模式（O12），P2 |
| 多人页 | 保留占位页（假数据、明确标注），P2 |
| 解压 | `zip` / `sevenz-rust` / `rars`（纯 Rust 库，外部 unrar 仅兜底） |
| .NET 元数据 | `dotnetdll`，解不出则降级只读 Assembly 版本 |
| 自研 | VDF 解析、SemVer 比较、皮肤合成（`image` + 自算变换） |
| 来源 | 用户来源表 > 目录字段 > Nexus id；GitHub 内置 tag 解析，其它来源用用户正则 |
| 整合包 | `CUPK1:` 排在 M8，先做来源解析与缓存 |
| 发布 | GitHub Releases：tag 触发双平台产物 + SHA-256，不做应用内自更新 |
| 不做 | 自更新、遥测、应用内卸载器、Supabase、插件系统、用户主题 |

## 4. 还没定的

1. 项目正式名与远程仓库：名字没定，先搁置；本地 jj 仓库不受影响，随时能接远程。
2. Windows 交叉编译与 CI 是否在 M0 就要。
3. 首次发布前要确认：是否把产物也传到 Nexus 页面（可选，不阻塞）。

## 5. 建议的下一步

1. **spike（已放行，交接后第一件事）**：HTTP 客户端来源、后台事件唤醒 UI、`Image` 显示内存位图 —— 任一条失败就要按结果改 `ARCHITECTURE.md` §9 再动手写代码。
2. 建 `crates/core`：`paths` → `settings` → `logs`，全程可单测，不碰 UI。
3. 建 `crates/app` 骨架：导航 + 像素壳 + `palette`，先做到「能起窗口、切页、缩放生效」。
4. 按 M1 起做本地模组（B、E1–E9、E17），这是产品成立的最短路径。

## 6. 接手须知

- 版本控制是 jj：`jj --no-pager st/desc -m .../new`，提交后 `jj bookmark set main -r @`；不要用裸 git 写操作。
- 注释政策是「默认零注释」：说明写进 `docs/` 镜像目录（`crates/core/src/mods.rs` → `docs/core/mods/overview.md`）。
- 造轮子、改架构、动依赖、碰认证/解包/删除/遥测之前，先按 `AGENTS.md` §2 提案等确认。
- 所有远端响应按不可信处理；破坏性操作两段式（先给计划，确认后才执行）。
- 写盘/删除/解压的改动必须在一次性目录验证，不要拿真实游戏目录试。

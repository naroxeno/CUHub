# 交接说明

截至 2026-11-14：core 基座已实现（paths/settings/logs、Steam 发现、modlist、catalogs、version、metadata、network、hubconfig），74 个单测通过；app 原型仍是六页壳，尚未接 core。spike 工程在仓库外 `/home/naroxeno/cuhub-spike`（nsh 0.5.1，7 页），只作参考。

## 1. 仓库现状

- jj 仓库（colocated，`main` 分支）；基础规则、app 原型与 core 分模块提交都在 `main`，`@` 是新的空 change。
- 骨架：根 `Cargo.toml` 是 virtual workspace（`crates/core` + `crates/app`），根 `nsh.json` 指向 `crates/app`；`crates/app` 是 nsh 根组件 `App` 与 `main.rs` 的 `run_app`；像素字体在 `crates/app/assets/fonts/`。
- core 模块：`paths`、`settings`、`logs`、`steam`（VDF + 库发现）、`modlist`、`catalogs`、`version`、`metadata`、`network`、`hubconfig`；`platform` 只有 `linux`/`windows` 同名函数；无 UI 依赖。
- app 原型（2026-11-14）：六页占位、1x/2x/3x 即时缩放、14 token 调色板、开发者控制台；设置持久化尚未接 core，重启回默认。
- 上游源码快照在 `ref/upstream/`（已进 `.gitignore`，不提交、不发版）。

## 2. 已有文档

| 文件 | 内容 |
|---|---|
| `AGENTS.md` | 代理规则：决策权、先问再做、注释政策、文档渐进披露、架构文档唯一位置 |
| `docs/FEATURES.md` | 全量功能清单（A–U），含像素风外观系统、来源表、里程碑与风险 |
| `docs/ARCHITECTURE.md` | 架构：两 crate、Hub 门面、事件、线程、外部边界、持久化、spike 结论与剩余未决 |
| `docs/app/nsh-notes.md` | nsh 0.5.1 的 API 与坑：工程骨架、列表/浮层、图片、任务、网络、窗口、测试、打包 |

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
| appdata | 目录名 `CUHub`（`~/.local/share/CUHub`） |
| 设置 | JSON 键 camelCase；解析失败不覆盖原文件；`uiScale` 夹 1–3 |
| 网络 | `core::network` 是唯一联网层（`reqwest` blocking）；`app` 不联网，封面由 core 落到 `IconCache` |
| 目录 | `catalogs` 只读 `Data/Catalogs/` 与旧 `Services/`；内嵌 AGPL 数据未搬 |
| 公告 | `hubconfig` 按版本选 stable/prerelease，缓存 30 分钟，只保留 https 链接 |
| 不做 | 自更新、遥测、应用内卸载器、Supabase、插件系统、用户主题 |

## 4. 还没定的

1. 项目正式名与远程仓库：名字没定，先搁置；本地 jj 仓库不受影响，随时能接远程。
2. Windows 交叉编译与 CI 是否在 M0 就要。
3. 首次发布前要确认：是否把产物也传到 Nexus 页面（可选，不阻塞）。
4. `core::network` 目前是 blocking API；`ARCHITECTURE §2` 说 Hub 层 async。拟由 Hub 把阻塞调用放进 worker，等确认。
5. 下载是否也归 `core::network`：m00262 说不放 core，m00281 又说 network 放 core；当前 network 已含 `download`，边界待确认。
6. 上游代码/目录 JSON 是 AGPL-3.0，本仓库尚无 LICENSE；能否搬运内嵌目录与离线公告待确认。
7. Windows 注册表（`winreg`）、macOS Steam 候选根、`GameLaunchService` 归哪个模块、来源表 `sources.json` schema 均未定。

## 5. 建议的下一步

1. **core 基座（已完成 2026-11-14）**：paths → settings → logs → steam → modlist → catalogs → version → metadata → network → hubconfig；单测 74 个通过，`cargo check --workspace` 通过。
2. **下一步（需先确认边界）**：`Hub` 门面（持有 settings/catalogs/network/logs，串行化网络 worker）与 `mods` 只读扫描（Plugins 列目录、DLL 版本证据、启用/禁用先不做）。破坏性/认证/解包模块（archive、protect、backup、nexus_auth、nexus）等确认后再动。
3. `crates/app` 原型：设置持久化、窗口尺寸/标题栏仍未做；接 core 时只读 core 状态，不联网。
4. 按 M1 起做本地模组（B、E1–E9、E17），这是产品成立的最短路径。

## 6. 接手须知

- 版本控制是 jj：`jj --no-pager st/desc -m .../new`，提交后 `jj bookmark set main -r @`；不要用裸 git 写操作。
- 注释政策是「默认零注释」：说明写进 `docs/` 镜像目录（`crates/core/src/mods.rs` → `docs/core/mods/overview.md`）。
- 造轮子、改架构、动依赖、碰认证/解包/删除/遥测之前，先按 `AGENTS.md` §2 提案等确认。
- 所有远端响应按不可信处理；破坏性操作两段式（先给计划，确认后才执行）。
- 写盘/删除/解压的改动必须在一次性目录验证，不要拿真实游戏目录试。

## 7. 工具链与 cargo（2026-11-14）

- 本机两套 cargo 并存，版本不同：
  - `~/.cargo/bin/cargo`：`1.101.0 (e611c7a87 2026-11-04)`，自报 `huacnlee/cargo 0.102.0-9`（<https://huacnlee.github.io/cargo/>）。Naro 的交互 shell 默认用这个。
  - `/usr/sbin/cargo`、`/usr/bin/cargo`：rustup shim，落到 `1.102.0-nightly (ede6cefea 2026-11-11)`；rustc 是 `1.102.0-nightly (8b778d04 2026-11-12)`。
- 两条都能编译 nsh 0.5.1 的 spike（edition 2024）；差别只在工具链版本，不在项目配置。
- 工具 shell 的 `PATH` 里 `/usr/sbin` 排在 `~/.cargo/bin` 前面，会抓到 nightly；Naro 的交互 shell 抓到 fork。**报告构建结果时注明用的哪一个**（`cargo -Vv` 第一行可区分）。
- 同一份代码两边结果不一致时，先查这里，再怀疑代码。

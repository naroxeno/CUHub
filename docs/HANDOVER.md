# 交接说明

截至 2026-11-15：core 基座与 GitHub 源的离线层、Check 接线已完成（github/sources/source_choice 模块，Hub 门面与 Local Mods 第三行），核心 165 个单测通过；禁用/启用与批量开关（FEATURES E6/E8）以及删除（E8 Delete all、E9 单项删除）已在 core 落地，app 已接（删除走两段式确认弹窗，列出绝对路径与文件数）；install 核心（zip/7z/rar 与裸 DLL staging、归属计划、备份/替换/回滚）与 Hub/UI 接线已落地（OutOfDate 或 GitHub release 版本更高的模组可 Update：选源、下载、解包、确认计划、Apply，游戏运行时排队）；app 的 Local Mods、Settings（含 Nexus key 保存/清除）、Nexus 目录列表（分页、下载入口、后台与手动刷新）与 Launch Game 已接 core，真实游戏目录验证能发现 2 个模组与 DLL 版本/GUID、反查 app id 4576490 并持久化设置。Nexus 直连下载尚未用真实 key 跑通，见 §4；GitHub 源还差远程清单仓库 URL，pin 按钮已落地，见 §4 第 10 条。spike 工程在仓库外 `/home/naroxeno/cuhub-spike`（nsh 0.5.1，7 页），只作参考。

## 1. 仓库现状

- jj 仓库（colocated，`main` 分支）；基础规则、app 原型与 core 分模块提交都在 `main`，`@` 是新的空 change。
- 骨架：根 `Cargo.toml` 是 virtual workspace（`crates/core` + `crates/app`），根 `nsh.json` 指向 `crates/app`；`crates/app` 是 nsh 根组件 `App` 与 `main.rs` 的 `run_app`；像素字体在 `crates/app/assets/fonts/`。
- core 模块：`paths`、`settings`、`logs`、`steam`（VDF + 库发现 + appid 反查）、`modlist`、`catalogs`、`version`、`metadata`、`network`、`hubconfig`、`mods`、`launch`、`nexus_auth`、`nexus`、`hub`、`github`、`sources`、`source_choice`、`install`；`platform` 只有 `linux`/`windows` 同名函数；无 UI 依赖。
- 许可：仓库根 `LICENSE` 为 AGPL-3.0 全文，两个 crate 的 `Cargo.toml` 标 `AGPL-3.0-only`；与上游一致。
- app（2026-11-14）：六页壳、1x/2x/3x 持久化缩放、14 token 调色板、开发者控制台；Local Mods 读 `Hub::scan_mods`，Settings 写入 `Settings.json` 并可保存/清除 Nexus API key，Nexus 页读缓存目录并分页 50 条显示，提供 Open page/Download、Details（占位）与 Refresh catalogue；目录过期时启动后台刷新；启动时自动发现 Steam 安装。GitHub 源在 Local Mods 每行第三行显示仓库、匹配依据、check 结果与生效源，可手动 Check；OutOfDate 或 GitHub release 版本更高的行可 Update（未列入清单的模组同样适用），下载归档后先展示计划再 Apply，游戏运行中排队。Multiplayer、Skins & Backups 仍是占位。
- 上游源码快照在 `ref/upstream/`（已进 `.gitignore`，不提交、不发版）。

## 2. 已有文档

| 文件 | 内容 |
|---|---|
| `AGENTS.md` | 代理规则：决策权、先问再做、注释政策、文档渐进披露、架构文档唯一位置 |
| `docs/FEATURES.md` | 全量功能清单（A–U），含像素风外观系统、来源表、里程碑与风险 |
| `docs/ARCHITECTURE.md` | 架构：两 crate、Hub 门面、事件、线程、外部边界、持久化、spike 结论与剩余未决 |
| `docs/app/nsh-notes.md` | nsh 0.5.1 的 API 与坑：工程骨架、列表/浮层、图片、任务、网络、窗口、测试、打包 |
| `docs/GITHUB_SOURCE.md` | 下一会话交接：单独仓库维护的 GitHub 模组清单、待拍板项与 MVP 顺序 |

## 3. 已拍板的决定

| 项 | 决定 |
|---|---|
| 结构 | 两个 crate：`core`（无 UI）+ `app`（nsh），依赖单向 |
| 交互形态 | 方案 C：`Hub` 门面 + 最多 5 个事件变体，不用事件总线 |
| 许可 | 仓库与两个 crate 使用 AGPL-3.0-only（`LICENSE` 为 AGPL-3.0 全文） |
| 下载 | 模组文件下载也归 `core::network`；`app` 不联网 |
| 启动 | `core::launch` 经 `steam://rungameid/{appid}` 启动，失败回退 `steam -applaunch`；不直接跑 exe |
| 平台差异 | `core/src/platform/{windows,linux}.rs`，同名函数导出 |
| 外观 | 像素风固定调色板（14 个 token），无用户自定义主题；1x/2x/3x 整数缩放，默认 2x |
| 字体 | Fusion Pixel 12px（OFL），缺字回落 Ark Pixel，再回落等宽 |
| 无障碍 | 保留高对比模式（O12），P2 |
| 多人页 | 保留占位页（假数据、明确标注），P2 |
| 解压 | `zip` / `sevenz-rust` / `rars`（纯 Rust 库，外部 unrar 仅兜底） |
| 单模组升级 | 解压到 staging；只覆盖当前 mod 自己的文件，其他已存在文件冲突即停；新增文件允许；release 只发裸 DLL 时只替换文件名唯一匹配的已装 DLL；备份到 `<AppData>/Backups/<timestamp>`；游戏运行时排队；不做一键升级 |
| 禁用/启用 | core 只改 DLL 扩展名（`.dll` ⇄ `.dll.disabled`），预检后逐个 rename，目标存在即报错、失败回滚；批量按整棵 Plugins 递归处理；界面已接，变更期间相关按钮一起置灰 |
| 删除 | 只删 Plugins 的一级条目（目录或 DLL 文件），不碰 BepInEx/core、设置与受保护资源；界面弹窗先列出绝对路径与文件数；游戏运行时拒绝；不做备份 |
| .NET 元数据 | `dotnetdll` 0.4（GPL-3.0+，与 AGPL 兼容）读取 Assembly 版本与 `BepInPlugin` GUID/版本；单个 DLL 解析失败降级跳过 |
| 自研 | VDF 解析、SemVer 比较、皮肤合成（`image` + 自算变换） |
| 来源 | 用户来源表 > 目录字段 > Nexus id；GitHub 内置 tag 解析，其它来源用用户正则；独立 GitHub 清单的优先级待定 |
| 整合包 | `CUPK1:` 排在 M8，先做来源解析与缓存 |
| 发布 | GitHub Releases：tag 触发双平台产物 + SHA-256，不做应用内自更新 |
| appdata | 目录名 `CUHub`（`~/.local/share/CUHub`） |
| 设置 | JSON 键 camelCase；解析失败不覆盖原文件；`uiScale` 夹 1–3 |
| 网络 | `core::network` 是唯一联网层（`reqwest` blocking）；`app` 不联网，封面由 core 落到 `IconCache` |
| 目录 | `catalogs` 只读 `Data/Catalogs/` 与旧 `Services/`；内嵌 AGPL 数据未搬 |
| Nexus key | 用户自己的 key 存 `NexusApiKey.dat`/`.key`（AES-256-GCM + 0600）；空值清除；不进日志/仓库；直连下载只走官方 API v1；Nexus 页已有目录列表与下载入口 |
| GitHub 源 | 在 Nexus 之外新增 GitHub 源；清单由单独仓库维护（schema v1：schemaVersion/generatedAt/mods，条目 name+repo+match+可选 asset）；客户端只读清单与公开 release；匹配 guid > dll > name，name 仅兜底；两源都显示，默认最新版本，用户可 pin（`<AppData>/source-choice.json`）；release 不缓存，404 回退 prerelease；Check 不下载资产，Update 时按 asset、首个 zip/7z/rar 或裸 DLL 下载，计划确认后安装；不做一键升级 |
| 公告 | `hubconfig` 按版本选 stable/prerelease，缓存 30 分钟，只保留 https 链接 |
| 不做 | 自更新、遥测、应用内卸载器、Supabase、插件系统、用户主题 |

## 4. 还没定的

1. 项目正式名与远程仓库：名字没定，先搁置；本地 jj 仓库不受影响，随时能接远程。
2. Windows 交叉编译与 CI 是否在 M0 就要。
3. 首次发布前要确认：是否把产物也传到 Nexus 页面（可选，不阻塞）。
4. `core::network` 目前是 blocking API；`ARCHITECTURE §2` 说 Hub 层 async。拟由 Hub 把阻塞调用放进 worker，等确认。
5. 是否把上游的目录 JSON 与离线公告内嵌进仓库：许可证已解决，但是否搬运仍需拍板。
6. macOS Steam 候选根、来源表 `sources.json` schema 未定。
7. 目录刷新已接后台 worker，但 blocking 闭包持有 Hub 互斥锁（最长约 20s）；期间其它 Hub 操作等待。是否改为锁外抓取待定。
8. Nexus 直连下载还没有真实 key 的端到端验证；Nexus 页已有下载入口（有 key 走官方 API，无 key 打开 Files 页），Standard 账号会收到 403，需要 Premium。
9. Windows 上 `NexusApiKey.key` 仍靠用户配置目录 ACL，尚未用 DPAPI 包裹；跨平台迁移旧裸 DPAPI blob 也未实现。
10. GitHub 源：清单仓库 URL 与远程拉取未定（目前只读本地 `Sources/manifest.json`）；限流 reset 时间需要扩 `network::Fetch` 才能显示；资产下载与单模组升级已接（来源选择、staging、归属计划、备份/回滚、游戏运行排队），一键升级与恢复界面未做。

## 5. 建议的下一步

1. **core 基座与只读扫描（已完成 2026-11-14）**：paths → settings → logs → steam → modlist → catalogs → version → metadata → network → hubconfig → mods → Hub；96 个单测通过，`cargo clippy --workspace --all-targets -- -D warnings` 通过。
2. **app 对接（已完成 2026-11-14）**：Local Mods 真实列表、Settings 游戏目录与缩放持久化、Nexus 目录列表与刷新、Steam 自动发现；app 仍不联网。
3. **core 基座收尾（2026-11-14）**：`launch` 已接 Launch Game 并改为非阻塞，AGPL 许可证已落地；`dotnetdll` 读取 DLL 版本/GUID，Windows Steam 根接入注册表；真实游戏根反查 app id 4576490 验证通过。
4. **下一步**：审查 GitHub 源代码；模组详情页（封面缓存、BBCode 描述、来源表）、启用/禁用与删除的界面接线（已完成）、下载/导入、来源表与解压（边界未定）、Hub 锁外刷新。
5. **GitHub 源与单模组升级（2026-11-15 已落地）**：清单 schema、匹配、pin、本地清单、Hub check（含 prerelease 回退）、Local Mods 第三行与 Check 按钮、失败文案；行可 Update（`OutOfDate` 或 GitHub release 版本更高），按 pin / Premium Nexus / GitHub 选源，下载归档或裸 DLL 后展示计划再 Apply，游戏运行中排队。剩余：清单仓库 URL 与远程拉取、限流 reset 头、一键升级。
6. 按 M1 起做本地模组（B、E1–E9、E17），这是产品成立的最短路径；E6 与 E8 的批量开关、E8 Delete all 与 E9 单项删除已在 core 落地，界面接线（含删除确认弹窗）与 E7 禁用态视觉已完成；E10–E14 的三列布局与行内动作仍未做。

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
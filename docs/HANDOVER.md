# 交接说明

截至 2026-11-14：设计与文档完成；app 原型已跑起来（六页壳、1x/2x/3x 缩放、开发者控制台），`crates/core` 仍是空 lib。spike 工程在仓库外 `/home/naroxeno/cuhub-spike`（nsh 0.5.1，7 页），只作参考。

## 1. 仓库现状

- jj 仓库（colocated，`main` 分支），四个提交：规则与功能清单 → 像素风重写 → 架构文档 → 来源表设计。
- 骨架已落地：根 `Cargo.toml` 是 virtual workspace（`crates/core` + `crates/app`），根 `nsh.json` 指向 `crates/app`；`crates/core` 只有空 lib；`crates/app` 是 nsh 根组件 `App` 与 `main.rs` 的 `run_app`；像素字体在 `crates/app/assets/fonts/`。
- app 原型已搭起（2026-11-14）：`shell.rs`（顶栏、侧栏 6 入口、状态栏、开发者控制台）、`pages.rs`（6 页占位、设置页 1x/2x/3x 切换）、`palette.rs`（14 个 token）、`ui.rs`（像素字体、面板双框、按钮、`scaled` 缩放 helper）。缩放与页面状态只在内存，重启回默认；内容区包在 `scroll()` 里，3x 在矮窗口下可滚。
- 原型与文档已提交到 `main`（`jj log` 里「搭起 app 原型…」），`@` 是新的空 change；接手从 `crates/core` 的 `paths → settings → logs` 开始。
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
| 不做 | 自更新、遥测、应用内卸载器、Supabase、插件系统、用户主题 |

## 4. 还没定的

1. 项目正式名与远程仓库：名字没定，先搁置；本地 jj 仓库不受影响，随时能接远程。
2. Windows 交叉编译与 CI 是否在 M0 就要。
3. 首次发布前要确认：是否把产物也传到 Nexus 页面（可选，不阻塞）。

## 5. 建议的下一步

1. **spike（已完成 2026-11-14）**：工程在仓库外 `cuhub-spike`；结论写进 `ARCHITECTURE.md` §9，API 细节在 `docs/app/nsh-notes.md`。HTTP 客户端来源、后台唤醒 UI、`Image` 位图三项已有结论或明确未决项，剩下的未决见 §9。
2. 建 `crates/core`：`paths` → `settings` → `logs`，全程可单测，不碰 UI。
3. `crates/app` 原型（已完成 2026-11-14）：6 页占位、1x/2x/3x 即时缩放、14 token 调色板、开发者控制台；设置持久化、窗口尺寸/标题栏仍未做，M0 验收见 `docs/FEATURES.md` §5。已知：3x 在 1271×711 的平铺窗口下 Local Mods 四列会挤（窄宽适配留到列表页）；控制台是根视图末尾的 `absolute` 子层，不是 `overlay()`。
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

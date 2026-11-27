# core 自查表

2026-11-16 生成。配合 [ARCHITECTURE.md](../ARCHITECTURE.md) 读：先看 A 组（安全边界），D 组只决定要不要动结构，最后才谈改法。

用法：每条给「位置 / 要看什么 / 我的初判」。你自己审完后按编号回「过」或「不过」，我按「不过」的动手。下面所有行号以当前工作副本为准。

## A. 安全边界

**A1 路径包含判断依赖调用方补 symlink**
位置：`paths.rs` 的 `is_inside`（纯词法 normalize）；调用点 `install/plan.rs:535`、`install/plan.rs:546`、`install/apply.rs:160`、`mods.rs:313`、`mods.rs:366`。
要看：是否有哪一处「删/写」只靠 `is_inside` 而没有自己的 symlink 检查。
合格：每个落盘/删除动作都有独立符号链接检查，或前置路径解析。
初判：疑。目前 plan/apply/mods 都另查了，属于「能过但靠人记住」。

**A2 解包过滤是否覆盖全部格式**
位置：`install/archive.rs:279` 起（zip symlink 检查在 295）、`archive.rs:320` 起（7z anti-item 在 332）、rar 分支。
要看：三种格式是否都拒绝 symlink / anti-item / 重复名 / 超限；失败后 staging 是否清干净（`archive.rs:148`）。
合格：三种格式行为一致，无遗漏分支。
初判：过。

**A3 归档条目名规范化**
位置：`archive.rs` 的 `normalize_entry_name` / `record_entry`。
要看：`..`、绝对路径、盘符、NUL、仅大小写不同的重复项。
合格：全部拒绝，且大小写归一后判重。
初判：过（有 `UnsafeEntry` / `DuplicateEntry`）。

**A4 凭据存储的「加密」名不副实**
位置：`nexus_auth.rs:146`（`save_secret`）、`nexus_auth.rs:212`（`write_private`）。
要看：密钥文件与密文同目录时，AES-GCM 只防「拷走单个文件」；unix 靠 0600/0700，Windows 无 ACL；`write_private` 是 create+truncate，崩溃会留截断文件。
合格：模块名/文档不承诺「加密保护」；写入原子。
初判：不过（可接受，但要么改文档要么改实现）。

**A5 日志是否可能带凭据**
位置：`logs.rs`、`hub.rs` 各处 `logs.activity` / `logs.error`。
要看：key/token 是否可能被格式化进日志；错误串里是否带完整 URL（含查询串）。
合格：凭据永不进日志，URL 落日志前去掉 query。
初判：疑，需要逐条核。

**A6 网络硬化**
位置：`network.rs:258`（redirect）、`network.rs:280`（validate_url）、`network.rs:338`（read_limited）。
要看：重定向是否只守 https 与凭据、大小上限、临时文件 `create_new`。
合格：如上全部满足。
初判：过。唯一缺口：重定向后的 host 未做白名单（要不要做由你定）。

**A7 桥的监听面（W2/W3 已接线）**
位置：`bridge/server.rs`（固定端口监听）、`bridge/handshake.rs`（token 与握手）、`bridge/session.rs`（帧与心跳）。
要看：只听 `127.0.0.1:41297`、token 常量时间校验、帧上限 1 MiB、未知字段忽略。
合格：协议文档 [protocol.md](../bridge/protocol.md) 的约束在代码里都有对应校验。
初判：过（固定端口、2 秒握手超时、1 MiB 帧上限、单会话均有校验）。

**A8 删除作用域**
位置：`mods.rs:288`（`delete_roots`）、`mods.rs:313`（`ensure_direct_child`）、`mods.rs:276`（`delete_all`）。
要看：`ensure_direct_child` 限制在 plugins 直属子项；`delete_all` 会删 plugins 下所有条目（含非 mod）。
合格：`delete_all` 的范围在界面上写明，不只说「全部模组」。
初判：过（语义要在文案里说清）。

## B. 数据完整性

**B1 settings 落盘无 fsync**
位置：`settings.rs:132`。
要看：tmp + rename 是否 fsync 文件与父目录。
合格：tmp + rename 后 fsync 文件与父目录（参考 `crates/core/src/atomic.rs`）。
初判：不过。

**B2 凭据写入非原子**
位置：`nexus_auth.rs:212`。
要看：`create + truncate` 改一半崩溃会丢凭据。
合格：tmp + rename（或 write_atomic）。
初判：不过。

**B3 四个 write_atomic 各写一遍**
位置：`metadata.rs:470`、`modlist.rs:263`、`hubconfig.rs:335`、`source_choice.rs:103`。
要看：四份实现的 fsync、临时名冲突、父目录创建是否一致。
合格：至少语义一致；能否合成一处由你定。
初判：疑（重复 + 不一致）。

**B4 备份路径 strip_prefix 退化**
位置：`install/apply.rs:215`（`backup_path`）。
要看：`strip_prefix` 失败时 `directory.join(absolute)` 会写出备份根。
合格：失败即报错，不退化成绝对路径。
初判：不过（潜在）。

**B5 备份与回滚顺序**
位置：`install/apply.rs:87` 起。
要看：先备份后写、回滚逆序、回滚失败是否报告。
合格：失败可见、目标可恢复。
初判：过。

**B6 hubconfig 两份缓存分开写**
位置：`hubconfig.rs` 的 config 缓存与 HTTP state。
要看：两者会不会读到「配置是新、状态是旧」的组合。
合格：可接受则写明；否则合并成一次写。
初判：待议。

**B7 BepInEx 缺失时设不了游戏目录**
位置：`paths.rs:68`（`game_paths` 要求 BepInEx 与 Plugins 都在）、`hub.rs:358`（`set_game_folder`）、`hub.rs:499`（`scan_mods`）。
要看：干净游戏（没装 BepInEx）能不能被设为游戏目录；`install/bepinex.rs:100` 的 `detect` 本来能表达「没装」，但门禁层先拦了。
合格：`game_paths` 只校验游戏根，缺 BepInEx 交给 `detect` 表达，Plugins 由安装流程创建。
初判：不过（vanilla 用户第一步就卡死）。

## C. 内存与资源

**C1 DLL 读取无上限**
位置：`mods/dotnet.rs:14`（`std::fs::read`）。
要看：单个 DLL 上限、插件目录总上限。
合格：超限直接跳过并记日志。
初判：不过。

**C2 扫描时逐个读 DLL**
位置：`mods.rs` `scan`。
要看：大目录下扫描耗时与内存峰值；是否每次都全量读。
合格：有上限或按需读。
初判：疑（与 C1 同源）。

**C3 日志每次全量克隆**
位置：`logs.rs:43`（`entries`）、`hub.rs:977`（`log_entries`）。
要看：UI 是否每帧调用。
合格：按需取尾部若干行，或返回借用。
初判：疑。

**C4 缓存文件读取无上限**
位置：`metadata.rs:322`（`load_cached`）。
要看：读整份缓存文件前是否核对大小。
合格：与网络上限一致。
初判：待议。

## D. 命名与边界

**D1 install 模块职责过宽**
位置：`install/` 下 archive、bepinex、process、plan、apply。
要看：解包与进程检测是否该独立成 `archive` / `process`。
初判：不过（名不副实）。

**D2 四个「来源」概念**
位置：`metadata.rs`、`sources.rs`、`source_choice.rs`、`catalogs.rs`。
要看：能否分别叫 社区目录 / 发布源清单 / 来源偏好 / 兼容性表。
初判：不过（一眼分不清）。

**D3 Hub 门面**
位置：`hub.rs:246`（字段）、`hub.rs:262`（`open`）、57 个 pub 项。
要看：拆不拆、`open` 是否该叫 `load` / `init`。
初判：不过（风险大，建议只改名不拆）。

**D4 展示层漏进 core**
位置：`mods.rs:111` 起、`metadata.rs:137` 起、`modlist.rs:46`（英文句子 Display）。
要看：与 app 的 `i18n::status_detail_key`（`crates/app/src/i18n.rs:74`）是否两套并存；`status_label` 是否无人调用。
初判：不过（i18n 会被卡住）。

**D5 Settings 混 UI 字段**
位置：`settings.rs`（`dashboard_*`、`easter_eggs_enabled`、`high_contrast_mode`、`local_mods_share_column_*`）。
要看：这些是否该去 app 侧状态。
初判：不过。

**D6 HubVersion 名字**
位置：`version.rs:6`。
要看：通用 SemVer 类型挂品牌名是否合适。
初判：不过（小改）。

**D7 linux.rs 其实是「非 Windows」**
位置：`platform/linux.rs:1`、`platform/linux.rs:45`（macOS 走 `open`）。
要看：改名 `unix.rs` 或拆 macos.rs。
初判：不过。

**D8 bridge 与 CUCoreLib 撞名**
位置：`bridge/`、[protocol.md](../bridge/protocol.md)。
要看：CUCoreLib 文档里 "bridge" 指它自己的 KrokMP 集成。
初判：不过（文档歧义）。

## E. 平台

**E1 进程检测靠外部命令**
位置：`install/process.rs:6`（linux 扫 /proc、windows `tasklist`、macos `pgrep`）。
要看：误判与命令缺失时的行为。
初判：疑。

**E2 启动路径的两个后台线程**
位置：`launch.rs:73`、`launch.rs:82`。
要看：线程失败无人知晓；app 退出即丢。
初判：待议。

**E3 macOS 缺口**
位置：`platform.rs`（无 macos 分支）、桥的 bottle 手填。
要看：macOS 是否只承诺「配置器 + 宿主」。
初判：待议（要写进文档）。

## F. 质量门

**F1 构建与测试**：`cargo fmt --check`、`cargo check --all-targets`、`clippy -D warnings`、`cargo test` 全绿。
**F2 没有靠 allow 压警告**：core 已确认 0 处 `#[allow(...)]`；app 未查。
**F3 生产路径无 unwrap**：core 仅 `test_support.rs` 一处；app 需确认。

## 建议顺序

A → B → C 是「真会伤数据/内存」，可小步分别提交；D 组是改名与搬代码，单独一轮，一次一个模块。E 组先写进文档再决定动不动。

## 修复状态（2026-11-17）

已修（含回归测试）：
- A1：`is_inside` 解析目录链的真实路径，叶子保留（链接叶子仍按「非普通文件」拒绝）。
- A4：`write_private` 改走 `atomic::replace_private`（临时文件 + fsync + rename，0600）；保护说明与模块文档不再承诺「加密保护」。
- B1、B2、B3：所有落盘统一到 `core::atomic::replace` / `replace_private`，含 fsync 与父目录 sync；四处重复的 `write_atomic` 删除。
- B4：`backup_path` 返回 `Result`，越界报 `Outside`，不再退化成绝对路径。
- B7：`game_paths` 只校验路径本身；`scan_mods` 遇缺 Plugins 返回空表；vanilla 游戏根可选，Plugins 由安装流程创建。
- C1、C4：DLL 读取上限 64 MiB（`MAX_DLL_BYTES`），缓存读取走 `read_limited`（8 MiB，与网络一致）。

核对后保持原判：
- A2、A3、A5、A6、A7、A8 过。A5 逐条核过 `logs` 调用：无机密值，无带查询串的 URL。
- B5 过。B6 保留两份缓存：HTTP state 只影响下次检查时机，不同步不伤数据。
- C2 接受：扫描必须读元数据，单文件上限已限住峰值。
- C3 接受：`log_entries` 无 UI 调用点，500 条上限固定。

未动，等拍板：
- D1–D7 命名与边界（拆 `install`、改四个「来源」、`Hub` 门面、展示层下沉、`Settings` 拆 UI 字段、`HubVersion` 改名、`linux.rs`→`unix.rs`）属架构变更，按 AGENTS §2.2 只留建议。
- D8 已在 `docs/bridge/protocol.md` 加歧义说明。
- E1 进程检测、E2 启动线程：行为未改，待议；E3 macOS 缺口已在 FEATURES §6 记录。
- F 组：`fmt`、`check --workspace --all-targets`、`clippy -p cuprum-core --all-targets -D warnings`、`test -p cuprum-core --lib` 全绿；core 0 处 `#[allow]`、生产路径零 `unwrap`；app 仅 `ui.rs:15,17` 两处 `#[allow(unused_imports)]`，无 `unwrap`/`expect`。

# 测试计划与覆盖矩阵

- 基线：`main` `1d0f4361`（2026-11-21）；工作副本为本波 W4 端口自动检测与交接改动（以 `jj st` 为准）。
- 结论：自动覆盖扩展到 core 377 单测 + 14 桥回环集成、app 25 条纯函数测试；真实游戏、双机、Windows、桥真机与打包安装仍没有可复现记录，真机工单见 `docs/bridge/w4-tickets/README.md`。

## 0. 现在到底测了什么

| 层 | 已有 | 仍缺 |
| --- | --- | --- |
| core | 377 个单测 + 14 个桥回环集成；fmt | 无属性/模糊测试；core clippy 有 2 个既有告警 |
| app | `nsh check`、25 个 `#[test]`（mp_control 15 条）、app clippy（2 条既有 lint 除外） | 无 UI 自动化；无 headless 自检 |
| 桥 C# | `dotnet build` 0 警告；`just package-bridge` 产出 0.1.1 zip | 未在 BepInEx / 游戏内加载 |
| 联机 | 房间码、参数、进程、状态、端口选择与交接有单测；页面截过图 | 无真实开房/加入、无双机、无 Windows |
| 内容库 | Store / Link / Profile / Adopt / Package 有单测 | 无真实游戏目录的端到端回归 |
| 打包 | `package-linux` / `package-windows` 配方存在 | 无安装、升级、卸载验证记录 |

## 1. 执行约定

- 破坏性用例只用临时游戏副本，不要拿真实游戏目录试删除、迁移、更新。
- 先备份 `~/.local/share/CUHub`；至少备份 `Settings.json`、`Store/`、`Profiles/`、`Links.json`、`Backups/`。
- 每次记录：提交号、系统、appdata 与游戏路径、操作、预期/实际、日志路径。
- 状态含义：`自动` 每次提交可跑；`单测` 只有 core 测试；`未测` 没有可复现记录；`未实现` 功能还不存在；`需环境` 当前机器做不了。
- 真机两机用例统一按 `docs/bridge/w4-tickets/README.md` 的 R1–R11 执行；偏差与裁定见 `docs/bridge/w4-tickets/decisions.md`，不要在执行时临时改脚本。
- 心跳集成测试的时序 flake 由 t15（只改测试）稳定；稳定前以暖缓存重跑为准。

## 2. 批次表

| 批次 | 内容 | 预计 | 何时跑 |
| --- | --- | --- | --- |
| A | 自动基线 | 5–10 分钟 | 每次提交 |
| B | Linux 首次启动、内容库、档案、迁移 | 1.5–2 小时 | 发布前 |
| C | 安装、更新 | 1–1.5 小时 | 发布前 |
| D | GitHub 源 | 0.5–1 小时 | 发布前 |
| E | 双机联机 | 1 小时 | 联机改动或发布前 |
| F | 桥真机 R1–R11（工单见 `docs/bridge/w4-tickets/README.md`） | 两机 1–1.5 小时 | 桥单独发布前 |
| G | UI、安全与 Linux 打包 | 1 小时 | 发布前 |
| H | Windows 实机与打包 | 2–3 小时 | Windows 发布前 |

## 3. 自动基线（批次 A）

| ID | 命令 | 通过标准 | 现状 |
| --- | --- | --- | --- |
| A1 | `cargo fmt --all -- --check` | 无 diff | 自动 |
| A2 | `nsh check` | 编译通过 | 自动 |
| A3 | `cargo test -p cuhub-core` | 377 单测 + 14 集成通过 | 自动 |
| A4 | `cargo test -p cuhub --all-targets` | 25 条通过（mp_control 15 条） | 自动 |
| A5 | `cargo clippy -p cuhub --no-deps -- -D warnings` | 只允许既有 2 条 lint（batch.rs:46 large_enum_variant、install.rs:244 never_loop）；其它告警判失败 | 自动 |
| A6 | `just build-bridge` | 0 error / 0 warning，插件 0.1.1 | 自动 |
| A7 | `cargo clippy -p cuhub-core --no-deps` | 已知 2 个既有 collapsible_if（hub.rs:823 / :1361），非门禁 | 非门禁 |
| A8 | `just package-bridge` | `dist/bridge/CuhubBridge.zip` 含 Plugin 与 Core 两 DLL | 自动 |
| A9 | `dotnet build mods/cuhub-bridge/CuhubBridge.Plugin/CuhubBridge.Plugin.csproj -c Release` | 0 警告 0 错误 | 自动 |

t7 基线（@ b679851c，父 1d0f4361）：10 条 exit 0；clippy-app 的 101 仅由 A5 里那两条既有 lint 造成；`cargo test -p cuhub-core` 冷构建首跑命中心跳 flake，暖缓存重跑全绿（t15 负责稳定）。

t18 基线（2026-11-21 12:50，游戏以 `steam -applaunch 4576490` 保持运行）：A1–A4、A6、A8、A9 exit 0；A5 仍为 101 且只由那两条既有 lint 造成；core 377 单测 + 14 集成，`bridge_server` 连续 10 次全绿。桥集成测试用 `start_at(0)` 绑临时回环端口，不占生产 41297、可与真实游戏并行（t16）；`close_session` 改为先取会话槽位并发 `SessionEnded` 再 shutdown，消除关闭/心跳竞态（`crates/core/src/bridge/server.rs`，t17）。

## 4. Linux 单机（批次 B）

| ID | 用例 | 验收点 | 现状 |
| --- | --- | --- | --- |
| B1 | 全新 appdata 启动 | 出现设置向导；不崩溃；标题栏可关闭 | 未测 |
| B2 | 未选游戏目录 | 明确空态或错误；不写游戏目录 | 未测 |
| B3 | 选游戏目录 + 默认 Store | 生成 `<appdata>/Store/mods`；重启保留 | 未测 |
| B4 | 自定义 Store 到另一块盘 | 写探针通过；重启保留；实际路径是 `<store>/mods`，不是 `Store/Store/mods` | 未测 |
| B5 | Store 只读或父目录不可建 | 拒绝保存并给可读原因；旧设置不变 | 未测 |
| B6 | 重跑设置向导 | 已保存设置保留；可改 Store；不重复下载 | 未测 |
| B7 | BepInEx 首次安装与修复 | 版本正确、文件完整、游戏能启动 | 未测 |
| B8 | 扫描散装 DLL、目录模组、`.disabled` | 行数、版本、启用状态正确 | 单测 |
| B9 | 单条与批量启停 | 散装条目改名正确；活动档案条目改 `enabled` 并解链/重建链接 | 单测 |
| B10 | 删除单条、范围、全部 | 两段确认；不穿过链接；不删游戏外文件 | 单测 |
| B11 | 新建空白档案、应用、删除 | `active.txt` 正确；删除不回滚已部署文件 | 未测 |
| B12 | 从当前部署创建档案 | 名称输入回写；entries 是当前部署快照；同名二次确认 | 部分 |
| B13 | Adopt 扫描、执行、再次扫描 | 备份到 `Backups/Adopt-*`；入库并建链接；再次扫描不重复处理 | 单测 |
| B14 | 托管条目启停与删除 | 页面显示「档案内」但按钮可用；删除=从档案移除并解链；禁用行保留 | 待补 |
| B15 | 档案应用失败 | 游戏运行或目标冲突时不写 `active.txt`，错误可读 | 单测 / UI 未测 |

## 5. 安装、更新与回滚（批次 C）

| ID | 用例 | 验收点 | 现状 |
| --- | --- | --- | --- |
| C1 | 远程新装 GitHub 模组 | pending → apply → Store + 活动档案 + Links；游戏可见 | 未测 |
| C2 | 依赖阻断 | 不进入 pending；装完依赖后手动重试 | 单测 |
| C3 | 本地 GitHub 更新 | 版本判定正确；新版本进 Store；链接重排 | 未测 |
| C4 | 托管模组更新 | 不再报 ManagedByProfile；档案条目替换；旧版本仍可查 | 单测 |
| C5 | 未托管目录更新 | 先备份到 `Backups/Package-*`，再收编并部署 | 单测 |
| C6 | 降级包 | 拒绝并清 staging；游戏目录与 Store 不变 | 单测 |
| C7 | 同版本不同字节 | 拒绝复用；已有 Store 内容不被覆盖 | 单测 |
| C8 | 第二份 pending | 丢弃新 staging；保留原 pending；提示先处理 | 单测 / UI 未测 |
| C9 | 取消、丢弃、游戏运行阻塞 | 取消后清 staging；游戏运行时保持阻塞，关游戏后重试；不产生半写状态 | 未测 |
| C10 | 坏包、路径穿越、超大文件 | 拒绝；不写出 Store / Plugins 之外；无残留 | 单测（部分） |
| C11 | 断网、GitHub 限流 | 可读错误；busy 释放；不循环重试 | 未测 |

## 6. GitHub 源与清单（批次 D）

| ID | 用例 | 验收点 | 现状 |
| --- | --- | --- | --- |
| D1 | 缓存与本地清单都缺失 | 远程页显示空态与清单路径；不崩溃 | 未测 |
| D2 | 当前 2 条清单 | 列表、搜索、过滤、已装匹配 | 未测 |
| D3 | 匹配优先级 | guid > dll > name；同一模组多条目取最优 | 单测 |
| D4 | Check、pin、来源选择 | pin 优先；只在用户操作时检查；不自动下载 | 单测 |
| D5 | release 缺失或无匹配资产 | 可读错误；精确 asset 缺失但唯一同形资产时回退成功；同形多资产仍报错 | 部分（选择逻辑有单测） |
| D6 | token、限流、缓存 | token 不进日志；限流提示；缓存命中 | 部分 |
| D7 | 远程清单拉取 | 默认 URL 条件请求、6h 缓存、启动后台刷新、缓存缺失回退本地；Remote Mods 手动刷新走 force；失败不清缓存 | 代码已实现，未实测 |
| D8 | 清单内容扩充 | 当前 435 B、2 条；真实匹配与更新覆盖不足 | 数据待办 |

清单扩充不是代码测试，但没有样本就无法验证 D2–D5。建议至少覆盖：单个裸 DLL、目录多 DLL、GUID 匹配、DLL 名匹配、仅名称匹配、asset 改名、无 release、坏条目。

## 7. 双机联机（批次 E）

| ID | 用例 | 验收点 | 现状 |
| --- | --- | --- | --- |
| E1 | 房间码格式 | 带或不带 `C1-`、大小写、空格、`-` 都能解析；坏校验拒绝 | 单测 |
| E2 | 开房 | core 启动；状态 WaitingPeer；房间码可显示/复制；默认端口 7790 | 未测 |
| E3 | 同 LAN 加入 | 5 秒内发现房主；带 forward 重启；Connected 显示 IP 与端口 | 未测 |
| E4 | 异 NAT / 中继 | 走 relay 能连；失败时有可读原因 | 未测 |
| E5 | 错误码与 60 秒超时 | 明确失败；给手动退路；不留进程 | 未测 |
| E6 | 端口占用与非法端口 | 明确错误；不崩溃；不占着旧端口 | 未测 |
| E7 | 离开后再次开房 | 进程、端口、状态、房间码都清理干净 | 未测 |
| E8 | EasyTier 未就绪与自定义目录 | 下载约 33 MB、sha256 校验；坏目录可读报错 | 未测 |
| E9 | Linux 无 CAP / Windows 非管理员 | Linux 由 core 的文件能力生效；Windows 显示 NeedElevation | 部分 |
| E10 | 游戏内看到房主并加入 | 游戏侧联机 mod 能通过 UDP 直连 | 未测 |
| E11 | 单隧道约束 | 第二次开房先停旧进程或拒绝，不并行两个 core | 单测 / UI 未测 |

## 8. 桥真机（批次 F）

批次 F 拆两段：8.1 每次改动都跑；8.2 等 Naro 约到搭档后两机执行。真机用例与工单同源：步骤、取证与注入见 `docs/bridge/w4-tickets/README.md`，偏差与裁定见 `docs/bridge/w4-tickets/decisions.md`；本表只保留编号与预期摘要。

### 8.1 自动（每次跑）

| ID | 用例 | 通过标准 | 现状 |
| --- | --- | --- | --- |
| F1 | Rust 宿主回环 | `cargo test -p cuhub-core` 的 14 条集成全过 | 自动 |
| F2 | 桥制品 | `just package-bridge` 生成 `dist/bridge/CuhubBridge.zip`，含 Plugin 与 Core 两 DLL，插件 0.1.1 | 自动 |
| F3 | C# 构建 | `just build-bridge` / `dotnet build -c Release` 0 警告 0 错误 | 自动 |
| F4 | 握手、权限、超时、单会话 | 第二连接被拒、超时清理、运行中授权立即生效 | 自动 |
| F10 | 事件消费 | `take_events` 仍未消费，状态只经 snapshot 轮询 | 未实现 |

### 8.2 真机（R1–R11，两机）

| 编号 | 用例 | 预期（摘要） |
| --- | --- | --- |
| R1 | 房主先开房、客人后进 | 60 秒内两端「已连接」；游戏内双向可见与开箱；端口与 `ss -lunp` 一致、来源「自动检测」 |
| R2 | 客人先等待、房主隧道后上线 | 客人 ≤30 秒自动变「已连接」，无需手动重试 |
| R3 | 桥卡片开关（默认关） | 关时无开房/加入按钮；打开 ≤4 秒生效；关闭后不再自动下发 |
| R4 | QuickTest 失败四组 | 缺 CUCoreLib →「游戏内联机不可用。」；端口占用 →「端口 N 已被占用。」/ 游戏内 `net_lnl_server_start_failed`；名称 <3 →「游戏会话不可用。」；非法端口 →「端口要在 1 到 65535 之间。」；房主只尝试一次，错误可读 |
| R5 | 游戏退出与重开 | 会话回「等待游戏连接。」、桥卡不报错；重开游戏自动再次开房 |
| R6 | 桥断开与重连 | 状态栏「桥：端口 41297 被占用」；释放后自动重连；插件 ≤30 秒重连成功 |
| R7 | 端口自动检测 | 7790 / 24444 两次都对上 `ss -lunp`；来源「自动检测」；手填在游戏未上报时标「手动」 |
| R8 | 游戏内直接开房 | 房间卡显示房主与 `10.144.144.1:<检测端口>`；`mp.host` 增量 0 |
| R9 | 换房 / 重进 | 旧隧道与旧端口清理干净；新码 ≤60 秒「已连接」 |
| R10 | 房主退出后客人侧 | ≤30 秒提示「房主已离开房间，等房主回来后可以重试。」；停止自动重试；可手动重试 |
| R11 | 房主换端口后客人侧 | 客人侧提示「房主已换房（游戏端口变了），请用房间码重进。」（或先落「房主已离开…」，以实际为准）；地址不显示旧端口；按码重进恢复 |

真机执行仍按 §1 的破坏性约定：只用临时游戏副本、先备份 `~/.local/share/CUHub`（至少 `Settings.json`、`multiplayer.json`）、每例记录提交号、系统、路径、操作、预期/实际与日志路径。

## 9. UI、安全与 Linux 打包（批次 G）

| ID | 用例 | 验收点 | 现状 |
| --- | --- | --- | --- |
| G1 | 无边框标题栏拖动、双击、最小化、最大化、关闭 | 行为正确；不误触；关闭前该确认的仍确认 | 未测 |
| G2 | 8 条缩放热区与最小尺寸 | 四边四角可缩放；不跳变；不盖住控件 | 未测 |
| G3 | 1x、1.25x、2x 缩放 | 标题栏、按钮、文字、房间码不重叠 | 未测 |
| G4 | 键盘 Tab、Enter、Esc 与中文输入 | 可完成主要流程；输入法不掉字 | 未测 |
| G5 | 9 项导航、返回、搜索 | 不丢状态；不出现空白页 | 部分 |
| G6 | 三语完整 | en / zh-CN / ja 无缺 key、无 raw key | 部分 |
| G7 | 长列表、50 行分页、滚动 | 不卡顿、不跳行、筛选正确 | 未测 |
| G8 | 复制房间码 | 剪贴板内容正确 | 未测 |
| G9 | `just package-linux` | 产物能启动；字体、资源、GTK/GStreamer 依赖正常 | 未测 |
| G10 | Linux 旧数据升级 | 不丢 Settings、Profiles、Store、Links、日志 | 未测 |
| G11 | 安全边界 | 路径穿越、symlink 逃逸、大小写冲突、坏 JSON、密钥/令牌日志全部拒绝且不泄漏 | 单测（部分） |

## 10. Windows 实机与打包（批次 H）

| ID | 用例 | 验收点 | 现状 |
| --- | --- | --- | --- |
| H1 | 非管理员启动并开房 | 状态卡显示 NeedElevation；提示以管理员重开；不弹 UAC 循环 | 未测 |
| H2 | 以管理员重开并开房 | core 启动；WaitingPeer；房间码正常；退出后进程清理 | 未测 |
| H3 | 无边框窗口 | 拖动、双击、最小化、最大化、关闭、四边四角缩放都正常 | 未测 |
| H4 | Windows 缩放 | 1x、1.25x、2x 下标题栏、按钮、文字不重叠 | 未测 |
| H5 | `just package-windows` 与 `Cuhub-setup.exe` | 安装、启动、卸载；无缺 DLL；快捷方式正确 | 未测 |
| H6 | Windows 旧数据升级 | 不丢 Settings、Profiles、Store、Links、日志 | 未测 |

## 11. 发布门槛建议

- 任何发布：批次 A 全绿。
- Linux 发布：B1–B14、C1–C11 与 D1–D5 的 P0 至少各跑一次；写盘用例必须在临时副本。
- 联机发布：E2–E10 在同 LAN 与异 NAT 各跑一次。
- Windows 发布：H1–H6 必过；G1–G3 也要在 Windows 看一次。
- 桥不随应用包发布；桥单独发布前 8.1 全绿，且 R1–R11 真机必过（工单见 `docs/bridge/w4-tickets/README.md`）。

## 12. 最该补的自动化

1. 实现 FEATURES Q7 的 `--selftest`：无显示器构建全部页面、向导与对话框；这是 app 目前唯一能低成本拿到的冒烟。
2. 把行模型、来源选择、错误映射这些纯函数从 UI 信号里拆出来做 app 单测。
3. 给压缩包路径、Links 索引、房间码加属性测试或模糊测试。
4. 在 CI 跑批次 A；Windows 至少加一个手动或虚拟机的启动冒烟。

## 13. GitHub 源未扩充（非测试待办）

`docs/GITHUB_SOURCE.md` 的代码侧（清单读取、匹配、pin、Check）已落地；远程清单拉取也已接（2026-11-19，`naroxeno/CUHub-Store` 根 `manifest.json`，6h 缓存，启动刷新，本地兜底）。但仓库当前为空，本机 `<appdata>/Sources/manifest.json` 目前只有 2 条，尚未扩充，所以当前只能测本地清单与 fixture，不能证明真实社区模组的匹配覆盖率。

扩充清单是数据/运营工作，不是本轮测试代码；建议单独列任务，扩到能覆盖第 6 节列出的样本类型后，再跑 D2–D5。

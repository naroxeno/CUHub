# 第三方来源台账：Terracotta

| 项 | 值 |
| --- | --- |
| 仓库 | burningtnt/Terracotta，https://github.com/burningtnt/Terracotta |
| 参考 commit | 694980b02c9ef3c4511f830e6554e3f12b42aef1 |
| 许可 | AGPL-3.0-or-later（与本仓库一致，允许移植） |
| 移植范围 | 仅 EasyTier 联机层中可按原样/近原样搬运的部分，落点为 crates/core/src/easytier/ |

## 逐文件台账

| 原文件 | 本库路径 | 改动等级 | 理由 |
| --- | --- | --- | --- |
| src/ports.rs | crates/core/src/easytier/ports.rs | 原样 | 端口号与回退偏移是跨端约定（RPC = 35780 + 判别值），必须与上游一致；仅新增文件头，并对暂未接线的 EasyTierRPC 抑制 dead_code。 |
| src/easytier/argument.rs | crates/core/src/easytier/argument.rs | 原样 + 补充 | 三个枚举逐字搬运；上游把参数拼装内联在 executable_impl.rs 中，这里抽成 command_arguments，flag 字符串与顺序不变，便于单测和进程层复用。 |
| src/easytier/executable_impl.rs（进程生命周期部分） | crates/core/src/easytier/process.rs | 改造 | 只取拉起、参数拼装、RPC 端口、stdout/stderr 转发、退出摘要、kill。可执行文件来源从「随包解压」改为 detect.rs 给出的安装路径，输出改走 crate::logs（上游走全局日志宏），并补上父进程死亡跟随（上游没有这项能力）。未移植：Android linkage_impl、Windows 控制台与窗口处理、MC 假服务器与扫描、add_port_forward。 |
| src/easytier/mod.rs（EasyTierMember/NatType 与 peer 解析） | crates/core/src/easytier/status.rs | 改造 | 数据模型照抄；解析从 UI 更新流程中抽出为独立的 parse_peers。未知 NAT 名上游在 debug 构建会 panic，本库一律返回 None。calc_conn_difficulty 未移植，它依赖上游 controller 的 ConnectionDifficulty，本库尚无对应类型。 |

## 只参考思路、本轮不落地的部分

| 原文件 | 计划落点 | 改动等级 | 说明 |
| --- | --- | --- | --- |
| src/controller/*（房间与成员状态机） | 待定 | 只参考思路 | 与上游 UI 和事件循环耦合，需按本库 hub/launch 边界重写。 |
| src/mc/*（MC 假服务器与端口扫描） | 不落地 | 只参考思路 | 本轮明确不移植。 |
| src/easytier/linkage_impl.rs | 不落地 | 只参考思路 | Android 专属。 |

## 单测与不可单测说明

| 本库文件 | 已覆盖 | 不可单测原因 |
| --- | --- | --- |
| ports.rs | 枚举判别值与回退偏移；回环端口分配 | 无 |
| argument.rs | 网络参数拼装；监听/端口转发/白名单参数拼装 | 无 |
| status.rs | parse_peers 对本地节点、远端节点、未知 NAT、坏 JSON 的处理 | easytier-cli 的真实输出格式属于集成测试，需要真实进程与网络。 |
| process.rs | 无单测 | 拉起真实 easytier-core 需要外部可执行文件、虚拟网卡与网络；退出摘要依赖真实进程时序；父进程死亡跟随依赖 OS 行为（Linux PR_SET_PDEATHSIG 需要父进程真实退出，Windows Job Object 需要 kill-on-close 触发）。参数拼装与端口分配已由 argument.rs/ports.rs 覆盖，故不引入 mock 框架。 |

## 依赖

未新增 crate 依赖。父进程跟随使用 std::process::Command::pre_exec 加手写 extern "C" prctl（Linux）与 extern "system" kernel32 声明（Windows）；JSON 解析沿用已有 serde_json。

## 接线缺口（交给开发者统一接线）

- crates/core/src/easytier.rs 需声明 argument/ports/process/status 四个子模块并转发对外类型，本轮未改该文件。
- process::spawn 需要 LaunchConfig，其 core/cli 路径来自 detect::Detection，现有对外接口保持不变。
- 端口转发目前只到 Argument 层，process 未提供 add_port_forward（上游用 CLI 注入，容易失败，等有需求再加）。
- status::parse_peers 的输入来自 process::peers_json 或 Hub 自己的 CLI 调用，二者尚未并入 hub 状态。

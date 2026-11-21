# 真机回归工单（W4 桥 + 游戏内联机）

从 `docs/bridge/w4-regression.md` 拆出（2026-11-21）。技术事实与端口机制仍在那份文件；本目录只放执行工单，判定以可观察信号为准：界面中文原文、`ss` 输出、Cuhub 活动日志与 BepInEx 日志。

## 导航

- 环境与硬前置、取证命令、注入与恢复：[commands.md](commands.md)
- 偏差与队长裁决（D1–D8）：[decisions.md](decisions.md)
- 用例（每例六字段：前置条件 / 步骤 / 预期 / 失败判据 / 证据文件 / 原始注入命令）：
- [R1](R1.md) 房主先开房、客人后进
- [R2](R2.md) 客人先进隧道、房主后开房
- [R3](R3.md) 桥卡片开关（默认关）
- [R4](R4.md) QuickTest 失败注入（4 组，各一次）
- [R5](R5.md) 游戏退出
- [R6](R6.md) 桥断开
- [R7](R7.md) 端口自动检测
- [R8](R8.md) 游戏内直接开房
- [R9](R9.md) 换房 / 重进
- [R10](R10.md) 房主退出后客人侧
- [R11](R11.md) 房主换端口后的客人侧

## 使用约定使用约定

- **角色**：A = 房主机器，B = 客人机器。两端装同一份 Cuhub、同一个桥插件包、同一份 CUCoreLib。
- **观察节奏**：Cuhub 轮询 2 秒（`crates/app/src/ui/bridge.rs:11`）；桥插件重连退避 1/2/5/10/30 秒（`docs/bridge/protocol.md:21`）；游戏端口采样 350 ms、连续 2 次确认、6 秒窗口（`crates/core/src/multiplayer/port.rs:7-10`）。所有「≤N 秒」判据都按这些周期留了余量，不要用秒表卡到正好 N 秒。
- **判定原则**：只认可观察信号（界面文案、日志行、`ss` 输出）。界面文案照抄 `locales/app.yml` 的中文原文。实现细节与该表冲突时，以代码为准并把冲突记进 [decisions.md](decisions.md)，不要私自改判据。
- **先取证再重试**：某步失败时先执行 `snap`，再重试；不要为了「再试一次能过」丢掉失败现场。
- **崩溃优先**：任何一端出现 panic 或应用退出，先拷 `$CUHUB_DATA/Crash Reports/` 再继续下一个用例。
- **单机/双机**：R3、R4、R6、R7 可在 A 单机完成；R1、R2、R5、R8、R9、R10、R11 需要 A、B 两台（R5 需要 A 能退出并重开游戏）。
- **客人端口**：新版客人留空时自动发现唯一房主的虚拟 IP 和端口，手填时限定端口。客人本地回环入口另行分配。旧客户端加入自定义端口房间时仍需手填。

## 覆盖矩阵用例 × 覆盖点 × 代码依据

| 编号 | 覆盖点 | 关键实现依据 |
| --- | --- | --- |
| R1 | 主链路：开房→加入→双向可见/开箱；端口来源与 ss 对账 | `crates/core/src/hub.rs:774-792`、`crates/core/src/hub/bridge_host.rs:165-189`、`crates/app/src/ui/mp_control.rs:113-176` |
| R2 | 客人先等待、房主隧道后上线 → 自动转「已连接」 | `crates/core/src/hub.rs:1012-1030`、`crates/app/src/ui/mp_control.rs:135-137` |
| R3 | 权限门控：默认关、开启立即生效、关闭后不再自动开房 | `crates/core/src/hub/bridge_host.rs:69-90`、`:330-340`、`crates/app/src/ui/mp_control.rs:247-281` |
| R4 | 四类失败面：缺依赖/端口冲突/名称过短/非法端口；重试门控 | `mods/cuhub-bridge/CuhubBridge.Plugin/MpBackend.cs:64-135`、`:179-201`、`crates/app/src/ui/mp_control.rs:18`、`:466-478`、`:205-211` |
| R5 | 游戏退出/重开：会话回 NoSession、桥不报错、可再次自动开房 | `crates/core/src/hub/bridge_host.rs:99-131`、`crates/app/src/ui/mp_control.rs:352-421` |
| R6 | 桥断开：端口被占可观测、释放后自动重连、插件退避 | `crates/core/src/bridge/server.rs:20`、`crates/app/src/shell/statusline.rs:81-83`、`docs/bridge/protocol.md:21` |
| R7 | 端口自动检测：来源标签、ss 对账、手填覆盖语义 | `crates/core/src/multiplayer/port.rs:7-42`、`crates/app/src/pages/multiplayer.rs:525-599` |
| R8 | 游戏内直接建房：Cuhub 不重复开房、隧道跟随端口 | `crates/core/src/hub.rs:936-999`、`crates/app/src/ui/mp_control.rs:395-421` |
| R9 | 换房/重进：旧隧道清理、无残留错误、新码可连 | `crates/core/src/hub.rs:813-816`、`crates/core/src/easytier/tunnel.rs` |
| R10 | 房主退出后客人侧：提示、停止自动重试、可手动重试 | `crates/app/src/ui/mp_control.rs:99-111`、`:490-507` |
| R11 | 端口变化后的客人侧：提示「换房/离开」、不显示旧端口、按码重进 | `crates/core/src/hub.rs:895-999`、`crates/core/src/room.rs:151-186`、`crates/app/src/ui/mp_control.rs:99-111` |

## 结果记录模板结果记录模板

每个用例执行完，在交付报告里贴一行（失败时多贴一列现象与证据目录）：

```text
R<n> | 结果(通过/失败/阻塞) | 执行人 | 开始时间 | A 版本 | B 版本 | 失败现象 | 证据目录
```

补充约定：

- 失败用例先交「现象 + 证据目录 + 复现步骤」，不要只贴结论；[decisions.md](decisions.md) 的 D 编号可直接引用（例如「失败，命中 D6」）。
- 同一用例重跑时证据目录加后缀（`R10-a2`），不覆盖第一次现场。
- 全部跑完后，把 11 行结果汇总成一张表，附在报告开头，再展开细节。

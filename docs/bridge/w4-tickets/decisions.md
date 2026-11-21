# 偏差与裁决（D1–D8）

本文件保存 `docs/bridge/w4-regression.md` §3 拆分前的偏差原文与队长裁决。原文不改判据；裁决决定每条按什么口径执行。总览见 [README.md](README.md)。

## 已知偏差与待裁决

真机执行前先读这一节。以下各条是「票面预期」与「当前实现/可注入性」不一致之处，判据按右列走，不要按票面原文判失败；每条都需要 Naro 或队长裁决是否算缺陷。

- **D1 重试次数与节奏**：票面写「0/3/8 秒退避 3 次」。实现里只有**客人**受 3 次限制（`crates/app/src/ui/mp_control.rs:466-478`、`:490-507`），**房主是一次性**（发起前置 gate.done，失败不重试，:476-478）；`Denied` 与 `invalid_params` 立即停止（:205-211）。R4 的房主用例只验证「只尝试一次」。
- **D2 端口占用的拦截层**：票面 4b 期望「桥回 unavailable + net_lnl_server_start_failed」。真机上「先占端口再开房」会先被应用层拦下（`Hub::start_host` 调 `ensure_port_available`，`crates/core/src/hub.rs:774-780`），文案是「端口 N 已被占用。」；桥的 `busy` 分支与游戏侧 `net_lnl_server_start_failed` 只能从游戏内 QuickTest 触发（§1.8 ④ 已实测）。R4-b 因此拆 b1/b2。
- **D3 三次退避在真机不可复现**：客人侧要进入自动加入流程必须先 phase Ready（隧道已连接），此时几乎不会连续失败；现有 UI 也没有稳定办法制造 guest 侧 mp.join 失败。该逻辑以单测（mp_control 15 条）为准，真机只验「不无限重试」。
- **D4 非法地址不可注入**：app 下发的地址恒为 `<ip>:<u16>`（`crates/app/src/ui/mp_control.rs:160-176`），端口输入框只收数字；桥的 `invalid_params`（名字/地址）真机 UI 无入口，由 t9 离线反射 harness 的 21 个用例覆盖。R4-d 改测应用层非法端口。
- **D5 UI 不展示具体失败原因**：桥卡提示只读桥自身的 `last_error` / `start_error`（`crates/app/src/pages/multiplayer.rs:113-124`），不展示 `mp.state` 的 unavailable 文本；R4-a 只能看到「游戏内联机不可用。」，R4-c 只能看到「游戏会话不可用。」，具体原因在 Cuhub 活动日志的 `Command mp.host failed:…` 行里。若要求界面直接显示原因，记为改进项。
- **D6 「手动」标签的触发条件**：端口来源以游戏上报值优先（`choose_host_port`，`crates/core/src/multiplayer/port.rs:25-42`）。游戏在房时手填只会出现「端口填的是 X，但游戏实际在 Y。」与「采用检测端口」，来源仍是「自动检测」；只有游戏未上报端口时才显示「手动」。票面「手填覆盖后标手动」只在后者成立。
- **D7 R6 重连窗口**：票面「10 秒内重连」。插件退避为 1/2/5/10/30 秒（`docs/bridge/protocol.md:21`），最坏要等满 30 秒；本工单按 ≤30 秒判，应用侧释放端口后 ≤5 秒（2 个轮询周期）自动重试。
- **D8 R2 严格路径不可达**：`Hub::start_host` 每次随机生成房间码（`crates/core/src/hub.rs:781`），界面不提供「按码开房」或「用上次房间码重新开房」（`LastRoom` 只持久化，未接 UI：`crates/core/src/multiplayer.rs:33-45`、`:93-96`）。因此无法「先让客人在某个码上等待、房主随后用同一个码开房」。若要覆盖这一场景，需要产品先补入口。

## 队长裁决（2026-11-21）

- **D1 接受（不算缺陷）**：按 11-20 决议——房主一次性、客人 0/3/8 三次；票面已按实现改写。
- **D2 接受**：占用优先由应用层 `ensure_port_available` 拦下，是端口优先级设计的一部分；b1 验应用层文案，b2 用游戏内 QuickTest 验桥 busy / net_lnl_server_start_failed。
- **D3 接受**：三次退避无法在真机稳定复现，以单测为准；真机只验「不无限重试」。
- **D4 接受**：非法地址 UI 不可达，由 t9 离线 harness 覆盖；R4-d 保持应用层非法端口。
- **D5 记为改进项，不阻塞本轮**：界面只显示桥自身错误、具体原因在活动日志；t8 已同步到 FEATURES 的后续改进，不实现。
- **D6 接受**：来源标签语义以 `choose_host_port` 为准（游戏上报优先）；「手动」只在游戏未上报时出现，票面按此改写。
- **D7 接受**：R6 以 ≤30s 为判据，备注应用侧释放端口后 ≤5s 重试。
- **D8 记为产品缺口，不阻塞本轮**：缺「按码开房 / 上次房间码重开」入口，R2 用可达路径；同时写入 FEATURES 改进项。

# multiplayer 模块

联机相关的少量持久化与展示状态；不放进程逻辑（那在 `easytier`）。

## 内容

- `MultiplayerState`：`schemaVersion`、`lastRoom{role,code}`、`corePath`、`relayOverride`；存 `<appdata>/multiplayer.json`。
- `RoomRole`：`host` / `guest`，序列化小写。
- `TunnelState`：UI 轮询用的摘要状态 `Idle / WaitingPeer / Connected{virtual_ip,port} / Failed(reason)`，由 `Hub::tunnel_state()` 计算。
- 游戏房间端口：`Hub::game_room_port()` 从 `mp.state` 的 `port` 取主机实际监听端口（`MpState::room_port()` 只在 `running && server` 时给值）；无游戏进程、桥未连、无会话或客人侧回 `None`。隧道侧的 `Connected{port}` 是隧道转发端口，与游戏房间端口不是一回事。

## 游戏房间端口

- 选定顺序：游戏上报的实际端口 > 手填端口 > 默认 `7790`；`0` 视为未填（`choose_host_port`，纯函数）。
- 占用检查：`ensure_port_available` 同时试绑 `0.0.0.0` 的 TCP 与 UDP，被占用或端口非法时返回 `PortError`，不静默换号。游戏已经上报该端口时跳过检查，不跟游戏自己抢。
- 变化跟踪：`PortWatch` 约 350ms 采样一次，连续 2 次读到同一新端口才确认，确认窗口 6s；超时或读不到端口时保持旧端口并只记日志（`PortUpdate::Unsettled`），不自动重试。
- 端口确认后 `Hub::restart_host_port` 用同一房间码重启隧道（网络名/密钥不变），成功回 `PortUpdate::Restarted`；失败回 `Failed`，端口保持旧值并尝试把隧道拉回旧端口。
- 错误分层：`HubError::Port` 是端口本身（非法/占用），`HubError::Tunnel` 是隧道重启失败，`HubError::GamePort` 是游戏实际监听与 Cuprum 选定端口不一致。
- `TunnelState::Connected{port}` 始终等于隧道当前端口：host 为白名单端口，guest 为发现到的房主端口。

## 边界

- 只存房间码与上次角色，不存密钥；密钥每次从码在内存派生。
- 写入：先写 `.json.tmp`，Unix 下 `0600`，再 rename；读失败或 JSON 损坏回默认值，不让启动失败。
- 房间事件日志只出现网络名前 4 位，EasyTier 的进程日志也会遮蔽房间网络名和密钥。
- `relayOverride` 为空表示用默认中继；`corePath` 为空表示用托管目录。

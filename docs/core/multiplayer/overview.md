# multiplayer 模块

联机相关的少量持久化与展示状态；不放进程逻辑（那在 `easytier`）。

## 内容

- `MultiplayerState`：`schemaVersion`、`lastRoom{role,code}`、`corePath`、`relayOverride`；存 `<appdata>/multiplayer.json`。
- `RoomRole`：`host` / `guest`，序列化小写。
- `TunnelState`：UI 轮询用的摘要状态 `Idle / WaitingPeer / Connected{virtual_ip,port} / Failed(reason)`，由 `Hub::tunnel_state()` 计算。

## 边界

- 只存房间码与上次角色，不存密钥；密钥每次从码在内存派生。
- 写入：先写 `.json.tmp`，Unix 下 `0600`，再 rename；读失败或 JSON 损坏回默认值，不让启动失败。
- 日志只出现网络名前 4 位，不输出 `lastRoom.code` 的密钥段。
- `relayOverride` 为空表示用默认中继；`corePath` 为空表示用托管目录。

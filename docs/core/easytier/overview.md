# easytier 模块

EasyTier 子进程与隧道门面。只负责「把内核拉起来、看懂它的状态、必要时重启一次」；不碰 UI、不做房间码 UI 校验、不代理游戏流量。

## 文件边界

- `release.rs`：版本 pin `2.6.4`、各平台资产名/大小/sha256、下载缓存 `<appdata>/Tools/easytier/<version>/`。
- `detect.rs`：探测托管目录或用户指定目录；Windows 必需 4 个文件（`easytier-core.exe`、`easytier-cli.exe`、`wintun.dll`、`Packet.dll`），其它平台 2 个。只读磁盘、不跑进程。
- `ports.rs`：按「本地回环 + 指定协议」申请空闲端口，用于 RPC。
- `argument.rs`：参数拼装；房间码只在这里派生成网络名/密钥，不落日志。
- `process.rs`：拉起 core/cli、接管 stdout/stderr、父进程退出时一起死（Windows Job / Linux PDEATHSIG）、`peers_json` 读取。
- `status.rs`：解析 `peer list -o json` 为 `EasyTierMember`，nat 类型未知时保留 `Unknown`，字段缺失返回 `None` 而不是猜。
- `tunnel.rs`：门面。host 加 hostname/静态 IP/端口白名单；guest 首次发现房主后带 `--port-forward` 重启一次，`forwarded` 标记防循环；找到房主后粘住，避免 peer 列表抖动导致状态回退。
- `mod.rs`（`easytier.rs`）：只做 `mod` 与 `pub use`，对外路径保持稳定。

## 边界与失败模式

- 提权：不自提权。`Hub::start_tunnel` 先查 `platform::elevated()`（Windows 管理员令牌；Linux `CAP_NET_ADMIN`），不足直接 `HubError::Elevation`。
- 内核缺失：先托管目录，再用户指定目录，最后按 release 下载；解压只取必需文件名，扁平写入缓存目录，缺一个即失败。
- 用户指定目录不完整时只记日志后回退下载，不删除用户文件。
- 隧道全程同一时刻只允许一个；重建前先停旧进程。
- 日志里房间码只出现网络名前 4 位与省略号；密钥段不落地。

## 常量出处

- 默认中继：`public.easytier.top:11010`、`public2.easytier.cn:54321`、`etnode.zkitefly.eu.org/node1|node2`（来自陶瓦 publics.rs，见 docs/third-party/terracotta.md）。
- Windows 资产约 32.7 MB，Linux 约 25.5 MB；sha256 在 `release.rs` 内 pin。
- 房主 `--hostname cuhub-host-<port>`，虚拟 IP 固定 `10.144.144.1`；端口白名单同时含 TCP 与 UDP。

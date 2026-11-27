# easytier 模块

EasyTier 子进程与隧道门面。只负责「把内核拉起来、看懂它的状态、必要时重启一次」；不碰 UI、不做房间码 UI 校验、不代理游戏流量。

## 文件边界

- `release.rs`：版本 pin `2.6.4`、各平台资产名/大小/sha256、下载缓存 `<appdata>/Tools/easytier/<version>/`。`ensure_archive_with_progress` / `ensure_installed_with_progress` 透传 `network` 的下载进度，旧的 `ensure_archive` / `ensure_installed` 以空回调委托。
- `detect.rs`：探测托管目录或用户指定目录；Windows 必需 4 个文件（`easytier-core.exe`、`easytier-cli.exe`、`wintun.dll`、`Packet.dll`），其它平台 2 个。只读磁盘、不跑进程。
- `ports.rs`：按「本地回环 + 指定协议」申请空闲端口，用于 RPC。
- `argument.rs`：参数拼装；房间码只在这里派生成网络名/密钥，不落日志。
- `process.rs`：拉起 core/cli、接管 stdout/stderr、父进程退出时一起死（Windows Job / Linux PDEATHSIG）、`peers_json` 读取。
- `status.rs`：解析 `peer list -o json` 为 `EasyTierMember`，nat 类型未知时保留 `Unknown`，字段缺失返回 `None` 而不是猜。
- `tunnel.rs`：门面。host 加 hostname/静态 IP/端口白名单；guest 在发现唯一房主后配置回环 TCP/UDP 转发，房主虚拟地址或端口变化时重建转发，连续三次失去候选才移除旧转发。
- `mod.rs`（`easytier.rs`）：只做 `mod` 与 `pub use`，对外路径保持稳定。

## 边界与失败模式

- 提权：不自提权。Windows 在 `Hub::start_tunnel` 先查 `platform::elevated()`（管理员令牌），不足直接 `HubError::Elevation`；Linux 不做进程级预检查，TUN 权限由 core 文件的 `cap_net_admin+ep` 在 exec 时授予，失败表现为 core 退出。
- 内核缺失：先托管目录，再用户指定目录，最后按 release 下载；解压只取必需文件名，扁平写入缓存目录，缺一个即失败。进度只覆盖压缩包传输：缓存命中且 sha256 一致时不会发请求，也就没有回调；解压与校验阶段同样没有进度。
- 用户指定目录不完整时只记日志后回退下载，不删除用户文件。
- 隧道全程同一时刻只允许一个；重建前先停旧进程。
- `Tunnel::restart(port)`：先停旧进程，再用同一房间码/中继按新端口重建。hostname 与端口白名单都随端口变化，所以端口变更只能靠重启。spawn 失败时 `port()` 保持旧值（进程已停），由调用方决定回滚（再重启旧端口）或报错。
- 启动日志只记内核路径与 RPC 端口；退出日志保存子进程输出前遮蔽房间网络名与密钥。

## 常量出处

- 默认中继：`public.easytier.top:11010`、`public2.easytier.cn:54321`、`etnode.zkitefly.eu.org/node1|node2`（来自陶瓦 publics.rs，见 docs/third-party/terracotta.md）。
- Windows 资产约 32.7 MB，Linux 约 25.5 MB；sha256 在 `release.rs` 内 pin。
- 房主 `--hostname cuprum-host-<port>`，虚拟 IP 固定 `10.144.144.1`；端口白名单同时含 TCP 与 UDP。

EasyTier 2.6.4 对端查询使用 `easytier-cli -p 127.0.0.1:<rpc> -o json peer list`。对端解析支持该版本的扁平数组和旧的 route 分组。

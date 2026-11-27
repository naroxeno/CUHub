# Cuprum 联机方案（EasyTier）与实施计划

状态：草案 v2（2026-11-16 按 Naro 拍板修改），等审计；只写计划，不含代码。

## 0. 结论

- 走 **EasyTier 内核子进程**方案：Cuprum 负责下载/校验/拉起/配置/状态，不自研 NAT 穿透，也不把内核编进 GUI；**安装包不带内核**，首次联机时在应用内下载，或指向用户自己装的 EasyTier。
- 房间码与房间协议**自己做一套**（不追求兼容 Scaffolding）：码里带网络名 + 密钥 + 校验；**mod 兼容交给联机 mod 自己检查**，Cuprum 不重复实现、也不因 mod 差异拒绝进入；MVP **不需要自建大厅或中继服务器**。
- **Windows 优先**，Linux 次之，macOS 最后（提权与签名成本最高）。
- 联机 mod 的 **Steam 与 UDP 两条路径都要留**：Steam 走 mod 自己，Cuprum 只读状态；UDP 直连走 EasyTier 虚拟局域网。
- 需要 Naro 拍板 7 件事，见 §10。

## 1. 需求与约束

- 游戏：Casualties Unknown（Steam appid 4576490），BepInEx 模组环境。
- 联机 mod 支持 Steam 与 UDP 直连两种方式。
- 目标体验：房主建房 → 得到房间码 → 好友输入房间码 → 进游戏能在局域网列表看到房间。
- 后续要求：与联机 mod 联动（FEATURES W3/W4 的桥命令已有位置）。
- 约束：app 不联网，联网/进程管理必须在 core；提权只能给独立子进程；破坏性操作与网络参数都要可见、可撤销。

## 2. Minecraft 启动器社区怎么做的（事实）

| 项目 | 做法 | 可借鉴点 |
|---|---|---|
| Terracotta（陶瓦联机） | 基于 EasyTier；房主建房得到房间码，输入即加入；内置 MC 服务扫描与「陶瓦联机大厅」；支持 Win/macOS/Linux/FreeBSD/Android；AGPL-3.0 + 例外（允许打包未修改二进制或经 HTTP API 交互） | 内核之上必须再包一层：房间码、进程管理、扫描、驱动与权限；上游许可允许聚合分发 |
| HMCL | 2025-10 起在启动器内接入 Terracotta/EasyTier 联机；文档解释了 TCP/UDP、IPv6、NAT 类型与中继 | 启动器负责体验，网络交给 EasyTier；IPv6 能显著提高成功率 |
| PCL2 CE / HMCL / FCL | 通过 Scaffolding 协议共享大厅编号，实现跨启动器联机；房间码形如 `U/NNNN-NNNN-SSSS-SSSS`，网络名 `scaffolding-mc-NNNN-NNNN`、密钥 `SSSS-SSSS`；联机中心用 hostname 发现，再走 TCP 交换信息 | **房间码 = 网络名 + 密钥 + 校验位**，零后端；公开格式让不同客户端互通 |
| EasyTierGame | 官方游戏联机 UI，自带内核；默认开启 WinIPBroadcast（UDP 广播），支持自定义配置 | UDP 广播中继是「游戏看不到房间」的关键；内核自带更新按钮但依赖外网 |
| SakuraFrp | 端口映射/内网穿透，MC 需要辅助 mod | 不是虚拟局域网，只作为对照组 |

来源：
- <https://github.com/burningtnt/Terracotta>
- <https://burningtnt.github.io/Terracotta/>
- <https://burningtnt.github.io/Terracotta/internal-protocol/>
- <https://docs.hmcl.net/multiplayer/feedback.html>
- <https://docs.pclc.cc/ce/feature/multiplayer>
- <https://easytier.cn/guide/gui/easytier-game.html>
- <https://doc.natfrp.com/game/mc>

### 2.1 从社区得到的六条教训

1. **内核用 EasyTier，体验自己做**：房间码、进程生命周期、驱动、提权、广播中继，少一个都会退化成「让用户自己配 VPN」。
2. **房间码是核心 UX**：把网络名和密钥编码进码里，加入方不需要账号、不需要大厅服务。
3. **LAN 发现比连通更难**：游戏靠 UDP 广播找房间时，必须处理广播中继（EasyTier 的 `--enable-udp-broadcast-relay` 仅 Windows、需管理员）。
4. **NAT/中继决定成功率**：需要默认引导节点，直连失败能走中继；IPv6 优先。
5. **不要自建专有大堂**：MC 启动器社区曾在联合公告后停掉各自联机，改投中立的 Terracotta/EasyTier。
6. **许可要提前想**：EasyTier 是 LGPL-3.0，下载分发同样要带许可证与源码地址；不修改二进制。

## 3. 与 Minecraft 的差异（对 Cuprum 的影响）

- CU 没有内置的 LAN 大厅协议；「房间」由联机 mod 定义（Steam lobby 或 UDP 端口），Cuprum 只能通过桥读状态。
- Steam 路径可能完全不需要虚拟局域网；Cuprum 不应强制装内核。
- 联机 mod 可能自带大厅/端口/房间码；若检测到 mod 已接管，Cuprum 退化为「启动 + 状态 + 引导」，避免两套隧道打架。
- 平台重心不同：Windows 是主战场；Linux 有 Steam/Proton，但 TUN 权限与广播中继都要另测；macOS 暂缓。

## 4. 方案选型

- **A. 应用内下载内核 + Cuprum 管理（推荐）**：安装包不增大；首次联机时下载（Windows zip 约 32.6 MB），版本 pin、sha256 可校验，失败可重试。
- **B. 用户自装 EasyTier/EasyTierGame，Cuprum 探测复用**：设置里可指定路径；版本/路径/驱动由用户负责，Cuprum 只做兼容性校验。
- **C. 把 `easytier` 作为库编进 core（不推荐）**：tokio 运行时、TUN 权限、崩溃面、版本漂移、macOS 签名，全部进 GUI；crates.io 版本还停在 2024-10。
- **D. 只做房间码 + 让用户自己开 EasyTier GUI（不推荐）**：体验割裂，无法自动化状态。

推荐：**A 为主，B 兜底**（二选一，设置里可切换）；MVP 只做 Windows。

## 5. 设计

### 5.1 core 新模块 `crates/core/src/easytier/`

- `release.rs`：pin 版本（2.6.4）、每平台资产名/大小/sha256、GitHub 下载地址；下载走 `core::network`；缓存到 `<AppData>/Tools/easytier/<version>/`；校验失败删除重下；下载入口只在用户点「下载」时触发。
- `detect.rs`：可选探测已装 EasyTier（PATH、常见安装路径、EasyTierGame 目录），比较版本；Windows 还需检查 `wintun.dll`、`Packet.dll`、`WinDivert64.sys` 是否在旁边。
- `config.rs`：把房间参数转成 CLI 参数或 TOML（`--network-name`/`--network-secret`/`--dhcp`/`-p peers`/`--rpc-portal`/`--hostname`/`--enable-udp-broadcast-relay`/`--no-tun` 降级）。
- `process.rs`：spawn/stop、stdout/stderr 落日志、退出清理（父进程死则子进程死）、端口冲突检测、启动超时。
- `status.rs`：`easytier-cli -o json node` 与 `peer list`（实测支持 JSON 输出），2 秒轮询；解析失败降级为 unknown，不 panic。
- `room.rs`：房间码编解码（见 5.2）、由码派生网络名/密钥、校验位。
- `easytier.rs`：对外 `Tunnel` 门面 + 事件（Started/Stopped/Failed/PeersChanged），与 Hub 的连接点。

### 5.2 房间码（自定，v1）

`C1-7K3M-9Q2X-4H8P-V6RW`：

- 字符表 32 个（`23456789ABCDEFGHJKLMNPQRSTUVWXYZ`，去掉 0/O/1/I/L），每字符 5 bit；共 16 个字符 = 80 bit。
- 前 8 字符是网络名（40 bit，解码为 `--network-name cuprum-XXXXXXXX`），接着 7 字符是密钥（35 bit），最后 1 字符是校验（5 bit，覆盖前 15 字符）；有效数据 75 bit。
- 不包含房主 IP/端口：双方各自向公共节点/对端发起连接，房主换网络不影响同一个码。
- 输入容错：大小写不敏感，粘贴时忽略连字符与空白；格式或校验错在本地直接报「邀请码不对」，不必等连接超时。
- 强度：密钥 35 bit（约 3.4×10^10），且要先知道网络名；房间是临时的，泄露只影响这一局。
- 前缀 `C1` 里的 `1` 是房间协议版本：以后改协议升到 `C2`，旧版本能明确提示「邀请码来自更新版本」，而不是连不上。
- 兼容性：借 Scaffolding 的思路，但格式自定，不追求互通；跨工具互通不在本期目标。

### 5.3 房间协议（最小化）

- 应用层没有自定义握手：房间码只负责派生 EasyTier 的网络名与密钥，连上就是同一张虚拟局域网。
- mod 检查由联机 mod 自己做：很多 mod 只影响客户端、不影响服务端，一刀切按「不一致」拒绝会误伤；重复实现也容易和 mod 的检查打架。
- Cuprum 的职责止于隧道：启动/停止内核、显示对端与连接状态；mod 报错时把它自己的提示原样展示（通过桥或日志），不替它下结论。
- 如果将来 mod 作者希望把检查结果前置到建房页，再按 §7 的桥扩展位单独讨论，不预付设计成本。

### 5.4 地址与网络参数

- 优先 `--dhcp`（EasyTier 自动分配，冲突会自动改 IP），失败再退回静态 `10.42.<hash>.<n>/24`。
- 默认开启 P2P；NAT 严格时走中继节点，默认内置 EasyTier 官方公共节点（高级设置可覆盖为自己的节点）。
- 本项目不提供后端服务；没有长期 VPS 也成立，房间码、发现、状态查询都在本机完成。
- 默认不启用魔法 DNS（会改系统 DNS），不安装系统服务，不开机自启。

### 5.5 提权

- Windows：独立 `easytier-core.exe` 通过 UAC（`runas`）拉起；Cuprum 主进程不提权；用户拒绝时给出「以管理员重试 / 取消」。
- Linux：首次提示 `setcap cap_net_admin+ep easytier-core`（一次性），或 `pkexec` 拉起；没有 polkit 时明确报错。
- macOS：后置；utun 需要 root，走 Authorization Services 或让用户手动装服务，成本高。

### 5.6 端口与发现

- RPC 只绑 `127.0.0.1`，端口从随机可用端口分配（避免 15888 冲突），状态查询用该端口。
- Windows 默认开 `--enable-udp-broadcast-relay`（需管理员 + WinDivert 驱动），解决游戏 LAN 广播找不到房。
- Linux/macOS 没有该开关：先在真机验证游戏是否只能靠广播；若只能广播，需要联机 mod 支持直连 IP/端口，否则该平台降级为「不支持局域网发现」。

### 5.7 打包与内核分发

- 安装包**不含** EasyTier 二进制；首次点「下载内核」时才拉 `easytier-windows-x86_64-v2.6.4.zip`（32.6 MB，解压后约 35 MB），校验 sha256 后放进 `<AppData>/Tools/easytier/<version>/`。
- 自装路径：用户已装 EasyTier/EasyTierGame 时，在设置里指定 `easytier-core.exe` 与 `easytier-cli.exe`；版本与伴随 DLL 检查通过才允许使用。
- Linux tar / macOS zip 同样不含内核，按需下载对应资产。
- 下载页与关于页展示 EasyTier 的 LICENSE（LGPL-3.0）与源码地址；不改二进制；sha256 写进代码。
- 不自动更新；内核版本随 Cuprum 版本 pin，更新时提示用户重新下载。

### 5.8 安全

- 房间密钥只存内存与 `<AppData>/multiplayer.json`（0600），不写日志、不进诊断报告。
- RPC 只监听回环；不开放公网；不启用魔法 DNS。
- 房间码输入做长度/字符集/校验位检查；CLI 输出按不可信输入解析。
- 提权只给核心进程；安装/更新内核必须 sha256 校验。
- 不绕过 Steam/游戏自身的认证与联机限制；Steam 模式不碰 Steam API。

## 6. UI（Multiplayer 页）

- 顶部模式选择：**Steam** / **虚拟局域网（EasyTier）**；未装内核时给出「下载内核（约 33 MB）」与「使用我已安装的 EasyTier」两个入口。
- 状态卡：缺内核 / 缺管理员 / TUN 驱动不可用 / 内核版本不匹配 / 端口冲突，每项带一个可执行动作。
- 房主：创建房间 → 大号房间码 + 复制；「高级」里可改中继节点、UDP 广播中继开关。
- 加入：粘贴房间码 → 校验 → 连接进度（启动内核 → 加入网络 → 等待对端 → 就绪）。
- 已连接：虚拟 IP、对端列表（主机名/IP/延迟/直连或中继）、离开房间、打开日志。
- 删除 R 段样例服务器与 R4 的假空态；文案不出现「内核/RPC/网络密钥」等实现词（沿用 BepInEx 文案原则）。

## 7. Steam 模式与 mod 联动

- Steam 模式：Cuprum 只负责启动游戏（`core::launch` 已有），Multiplayer 页通过桥 W3 显示房间/玩家；W4 的 host/join/leave 交给 mod。
- EasyTier 模式：Cuprum 管隧道；mod 仍负责游戏会话（host/join）。桥扩展位：
  - mod → Cuprum：`mp.state` 增加 `transport`（steam/udp）、`virtual_ip`、`lan_port`、`room_code`（可选）。
  - Cuprum → mod：`mp.host`/`mp.join` 带 `{transport, room_code, virtual_ip}`；control 档，默认关。
- 若 mod 自己就带大厅/隧道，Cuprum 只显示状态并提供启动，避免双隧道。
- 这部分依赖 mod 作者确认协议；本计划只定义扩展位，不实现。

## 8. 分阶段任务（不含代码）

1. **P1 内核获取与检测**：release 表、下载、sha256、缓存、版本/完整性检查、单测。
2. **P2 进程与状态**：spawn/stop、JSON 状态解析、轮询、退出清理、端口冲突、日志。
3. **P3 房间与配置**：房间码编解码、multiplayer.json、DHCP/静态 IP、peers、hostname。
4. **P4 提权**：Windows UAC helper；Linux setcap/pkexec；失败提示。
5. **P5 UI**：Multiplayer 页重写 + i18n（三语）。
6. **P6 打包与分发**：安装器不含内核；首次下载/校验/自装路径；LICENSE 与关于页；Linux/macOS 包。
7. **P7 真机测试**：双机、NAT、广播、断线；按 §9。
8. **P8 Steam/桥联动**：依赖 mod，先只做只读状态。

每阶段完成后再进下一阶段；P1–P3 可以在没有管理员权限的机器上用 `--no-tun` 做部分验证。

## 9. 测试计划

- 单元：房间码校验位、参数生成、状态 JSON 解析、路径与版本校验、提权失败分支。
- 集成：本机两个实例不同 network name，互相 ping；CLI 状态 JSON 解析。
- 双机：Windows+Windows、Windows+Linux；同 NAT、不同 NAT；错误房间码；断线重连；杀进程后 TUN 清理。
- 游戏：CU + 联机 mod，主机开房 → 客机在 LAN 列表看到；开/关广播中继对比；Steam 模式对照。
- 打包：干净 Windows 10/11 安装器；非管理员启动 → 引导提权；卸载残留检查；断网/校验失败/磁盘不足时的下载提示。
- mod 兼容：确认 Cuprum 不因 mod 差异拒绝进入；联机 mod 自己的检查按其 UI/日志走。
- 安全：确认日志无密钥、RPC 不外露、内核退出后无残留进程/网卡。

## 10. 风险与待拍板

风险：首次下载约 33 MB（解压后约 35 MB）；杀软可能误报 `WinDivert`；管理员门槛；游戏/联机 mod 的入站端口可能被 Windows 防火墙拦截，需要提示用户放行；Linux/macOS 广播发现不确定；公共中继节点可用性；EasyTier 迭代快（2.6.x）；LGPL 合规；自定码格式要自己维护版本演进；Steam 模式与 mod 自带大厅可能冲突。

需要 Naro 决定：

1. **平台顺序**：Windows 优先、Linux 次之、macOS 后置，可以吗？
2. **mod 检查结果**：完全交给联机 mod（Cuprum 不拦截），还是把它的结果只读转述到 Multiplayer 页？
3. **房间协议**：已确认自定（不兼容 Scaffolding）、不另设应用层握手，前缀 `C1`，OK 吗？
4. **默认中继节点**：默认内置 EasyTier 公共节点、不打算自建服务器（没有长期 VPS），可以吗？高级里仍允许用户覆盖。
5. **提权体验**：接受「点连接 → UAC/授权」一次吗？
6. **Steam 模式**：Cuprum 只做启动与状态，邀请好友完全交给 Steam/mod，行吗？
7. **FEATURES 的 R 段**：直接删掉样例服务器、改成真实联机页，优先级按 P1/P0 定？

参考来源见 §2；EasyTier 版本与资产事实：2.6.4，Windows zip 32,657,720 B、sha256 `27af91e2…5f84`，Linux zip 25,506,523 B、sha256 `61b659ea…dfd6`（本次下载实测，正式 pin 前再对官方 release 页核一遍）。

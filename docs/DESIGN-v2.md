# Cuhub v2 设计：联机与模组管理

状态：审核草案 v1（2026-11-17），只写设计，不含代码。审阅重点在 §7 的待拍板清单。
本文取代 `docs/multiplayer/easytier-plan.md` 的联机部分（那份留作背景与来源）；模组管理部分是新增。

---

## 0. 范围

| 主题 | 做 | 不做 |
|---|---|---|
| 联机 | EasyTier 子进程、房间码、房主发现、状态展示、与桥的接口 | UDP 广播中继、自建服务器、mod 兼容拦截、Scaffolding 互通、macOS |
| 模组管理 v2 | 内容库、链接部署、profile、整合包（manifest / embedded） | MO2 式 VFS、USVFS、服务器托管整合包、加载顺序控制 |

两件事共用一个前提：**游戏目录只放部署结果，内容库与配置都在 appdata**。这样卸载与切包都不再依赖「记得删哪个文件」。

## 1. 名词

| 名词 | 含义 |
|---|---|
| **Store（内容库）** | `<appdata>/Store/` 下的模组本体，按 `modId@version` 存放，只追加不原地改 |
| **Link（部署）** | 游戏目录里指向 Store 的条目：目录 junction / 硬链接 / 符号链接 / 拷贝 |
| **Profile** | 一份「装了什么、开什么、覆盖哪些 config」的清单，`<appdata>/Profiles/<name>/profile.json` |
| **Pack（整合包）** | 可分享的清单 + 覆盖文件；`payload=manifest` 只带来源，`payload=embedded` 连本体一起带 |

## 2. 联机

### 2.1 目标体验

房主：点「创建房间」→ 等内核就绪 → 得到 `C1/XXXX-XXXX-XXXX-XXXX` → 复制发给好友。
客人：粘贴码 → 连接 → 在游戏里直接连房主的局域网会话。全程不填 IP、不装 EasyTier、不建账号。

### 2.2 相对 easytier-plan 的三处修正

1. **不要 UDP 广播中继**：联机 mod 已支持 UDP 直连（填 IP + 端口即可入房），广播只是「局域网列表自动发现」，不是连通的前提。Windows 上因此只剩 TUN 提权一项，WinDivert 与 `--enable-udp-broadcast-relay` 整块删除。
2. **房主发现用 hostname，不用广播也不硬编码 IP**：房主进程以 `--hostname cuhub-host-<端口>` 启动，客人从 `easytier-cli peer list -o json` 里按 hostname 前缀认出房主，读出其虚拟 IP 与端口，把两者填进联机 mod 的直连框。这条路不绑定子网、不需要房主固定静态 IP。
3. **工时按「只剩提权」重估**：Windows MVP 20–30 小时（提权 6–10、子进程与状态 8–12）；Linux 再 +6–8。

### 2.3 core 模块 `crates/core/src/easytier/`

| 文件 | 职责 | 备注 |
|---|---|---|
| `release.rs` | 版本 pin 2.6.4、各平台资产名/大小/sha256、下载与缓存 | 已有；下载走 `core::network` |
| `detect.rs` | 探测自装内核、校验伴随 DLL（`wintun.dll`）与版本 | 已有；补版本比较 |
| `args.rs` | 生成 CLI 参数：网络名/密钥、hostname、RPC 端口、peers、dhcp | 新增 |
| `process.rs` | 拉起/停止、父死子死、端口冲突、启动超时、日志接管 | 新增 |
| `status.rs` | 轮询 `easytier-cli -o json node` 与 `peer list`，产出 `TunnelStatus` | 新增 |
| `room.rs` | 房间码 C1 编解码 | 新增 |
| `discovery.rs` | 从 peer list 按 hostname 找房主，返回 `(虚拟 IP, 端口)` | 新增 |
| `easytier.rs` | `Tunnel` 门面 + 事件，对 `Hub` 只暴露 start/stop/status | 收口 |

事件只保留两个：`TunnelReady`、`TunnelLost`。其余状态由 `Hub::tunnel_status()` 轮询返回，不新增事件类型（架构文档规定事件最多 6 个）。

### 2.4 时序

```
房主                                  客人
点创建房间                            粘贴码 → room::decode 校验
  确保内核（缺则下载/校验）             确保内核
  UAC/setcap 提权一次                  UAC/setcap 提权一次
  spawn easytier-core --hostname ...   spawn easytier-core
  status: Running                       status: 等待 peer
  ← 房间码（网络名/密钥编码在内）          discovery: 找到 cuhub-host-N
                                        → 得到 IP:端口 → 填进 mod
```

房间码里**不含地址**：码只派生网络名与密钥，所以房主重连、换节点都不影响同一个码。地址由 hostname 约定在运行时发现。

### 2.5 房间码 C1

沿用 `easytier-plan.md` §5.2：32 字符表 `23456789ABCDEFGHJKLMNPQRSTUVWXYZ`（去掉 0/O/1/I），16 字符 = 前 8 网络名 + 7 密钥 + 1 校验（5 bit XOR 覆盖前 15 字符），输入大小写不敏感、忽略 `-` `/` 与空白，`C1-` 前缀可省略。
新增两条约束：
- 码里显式带游戏端口不属于协议，端口只出现在 hostname；换端口不换码。
- 解码失败分三种提示：格式不对 / 校验位不对 / 版本更新（`C2` 前缀）——不能都说「连不上」。

### 2.6 房主发现

1. 客人 `peer list -o json` 取全部 peer 的 hostname 与虚拟 IP，路由状态只认 `p2p` 与 `relay` 两种。
2. 匹配 `cuhub-host-<port>` 前缀且端口在 1–65535，多个命中时报「房间有多个房主」而不猜。
3. 找不到时按 5 秒一次重试，60 秒后提示「房间存在但房主未就绪」并提供「手动填写地址」退路（保留粘贴 IP:端口）。
4. 拿到地址后不自动替用户点加入：联机 mod 的入房由用户触发，Cuhub 只负责把地址填好并高亮。

### 2.7 进程、提权、端口

- 内核进程由 Cuhub 拉起、Cuhub 退出即终止；不从 shell 继承 stdin，stdout/stderr 进 `Logs/`。
- 不自提权，也不在 Linux/macOS 做进程级预检查：Windows 用 `platform::elevated()`（`Shell32!IsUserAnAdmin`）检查当前进程，不足时 `start_tunnel` 返回 `HubError::Elevation`，界面提示「以管理员身份重新打开 Cuhub」；Linux 的 TUN 权限由 core 文件的 `cap_net_admin+ep` 在 exec 时授予，Cuhub 自己缺权不影响。不弹 UAC、不循环重试。
- Linux 用 `sudo setcap cap_net_admin+ep <core>` 后免 root；提示里给出这条指引，不自动执行。
- RPC 只绑 `127.0.0.1`，端口从系统分配的空闲端口拿，写进本次会话状态；不占固定 15888。
- 端口转发不做 RPC 补丁：陶瓦靠 easytier gRPC 动态加转发，本项目没有移植 linkage，客人改为「发现房主后带 `--port-forward` 参数重启一次」，`forwarded` 标记防循环；TCP 与 UDP 各一条，本地端口尽量等于房主端口。
- 同一时刻只允许一个隧道；重复创建房间时先停旧进程再起新的。

### 2.8 状态与错误

`TunnelStatus`：`Idle / NoCore / NeedElevation / Starting / WaitingPeer / Connected / Stopping / Failed(reason)`。

| 错误 | 文案要点 | 可执行动作 |
|---|---|---|
| `NoCore` | 还没下载内核（约 33 MB） | 下载 / 指定已装路径 |
| `NeedElevation` | Windows 当前进程不是管理员，无法建虚拟网卡 | 以管理员身份重新打开 Cuhub |
| `TunUnavailable` | 虚拟网卡驱动不可用（缺 WinTun） | 重新安装内核 |
| `PortInUse` | 端口被占用 | 换端口（自动选下一个） |
| `PeerNotFound` | 房间在线但找不到房主 | 重试 / 手动填地址 |
| `JoinTimeout` | 60 秒未拿到 peer | 查看日志 |

### 2.9 持久化 `multiplayer.json`（0600）

```json
{ "schemaVersion": 1,
  "lastRoom": { "role": "host", "code": "C1-7K3M-9Q2X-4H8P-V6RW" },
  "corePath": null,
  "relayOverride": null }
```
只存房间码与上次角色；**密钥只在内存里从码派生**，不落盘、不进日志（日志里输出网络名前 4 字符与 `…`）。

### 2.10 UI（Multiplayer 页）

替换现有 R 段样例数据：顶部模式切换（Steam / 虚拟局域网），下面一张状态卡（对应 §2.8），再下面是房主区（大号房间码 + 复制）或加入区（输入码 + 连接）。已连接时显示虚拟 IP、对端表（主机名/IP/延迟/直连或中继）、离开房间。
文案不出现「内核 / RPC / 网络密钥」，与 BepInEx 那边同一原则。

### 2.11 与桥的接口

`mp.state` 增加 `transport`（steam/udp）、`virtual_ip`、`lan_port`、`room_code`（可选）；`mp.join` 接受 `{transport, virtual_ip, lan_port}`，让玩家不必手抄地址。写侧依赖联机 mod 作者确认，属条件触发项，不计入 MVP；未确认前一律保留复制粘贴兜底。

### 2.12 测试

单测：房间码往返与校验位、CLI JSON 解析、hostname 匹配（含多命中与畸形）、参数生成。
集成：本机双实例不同网络名互 ping；杀内核后进程与网卡清理干净。
双机：Windows+Windows 同/异 NAT；Windows+Linux；错误码；断线重连。游戏内：房主开房 → 客人按发现的 IP:端口入房。

## 3. 模组管理 v2

### 3.1 现状与问题

现在模组直接解包进 `BepInEx/Plugins`，按文件名替换。三个结构性痛点：
1. 卸载只能删目录，删错不可恢复；
2. 同一个 mod 的两个版本不能共存，回滚要重新下载；
3. 分享「我这一套」只能发压缩包或让别人照列表手动装。

### 3.2 四个概念

Store 是唯一真源；Profile 只是一张清单；游戏目录里的东西全部是 Link；Pack 是清单的交换格式。
关系：`Pack →(导入)→ Store + Profile →(Apply)→ Links`。

### 3.3 目录布局

```
<appdata>/Store/mods/<modId>/<version>/
    payload/            # 解包后的文件（原样，不改）
    entry.json          # 来源、sha256 清单、安装时间、插件 GUID
<appdata>/Store/blobs/<sha256>/…    # 可选：文件级去重（v2 不做，留位）
<appdata>/Profiles/<name>/profile.json
<appdata>/Links.json                 # 部署索引：游戏目录路径 → modId@version + 链接类型
<appdata>/multiplayer.json
```

`modId` 优先取插件 GUID，没有则用规范化目录名（复用 `install::normalize_install_name`）。`version` 取 BepInPlugin 版本，取不到记 `unknown`。

### 3.4 部署单元与链接策略

BepInEx 的 Plugins 顶层就是「一个目录或一个 DLL = 一个模组」，所以部署单元只有两种：

| 单元 | 首选 | 次选 | 兜底 |
|---|---|---|---|
| 目录型模组 | 目录 junction（Windows）/ symlink（Unix） | —（junction 可跨卷） | 拷贝 |
| 单文件 DLL | 硬链接（同卷，无需管理员） | 符号链接（Unix；Windows 需开发者模式） | 拷贝 + 界面标「copied」 |
| `BepInEx/config/*.cfg` | 逐文件硬链接 | 符号链接 | 拷贝 |

选择理由：目录型占绝大多数，junction 一劳永逸解决跨卷；只有单 DLL 模组在异卷时才退化。退化必须在界面上写明，不能静默拷贝（用户会以为改库生效了）。

### 3.5 部署算法 `apply_profile`

```
1. desired = profile 里 enabled 条目 → [{路径, 目标, 类型}]
2. current = Links.json 里属于本 profile 的条目
3. 计划 = create / remove / replace 三类差异（不碰 Links.json 里不属于本工具的条目）
4. 执行顺序：先建新链 → 再删旧链 → 最后写 Links.json
5. 任一步失败：逆序回滚已建链接，界面上报「部署未完成，已回到开始前状态」
```

要移除的链接：只删链接本身（`remove_dir` 对 junction 只解链）。目录里若有用户手动放进的东西，先列出来再决定，不递归删。

### 3.6 删除与安全

- **不递归删除游戏目录**：删除一律走 Links.json 的索引，逐个解链；`remove_dir_all` 只对 Store 内的路径使用。
- 所有目标路径 `strip_prefix` 后必须只剩一个普通组件，且解析后仍在 Plugins / config 内。
- 共享引用检查：删某个 `modId@version` 前先扫所有 profile 与 Links.json，仍被引用则只提示、不删。
- 硬链接无法反查来源，所以 Links.json 是唯一权威；索引与磁盘不一致时以磁盘为准重建（`verify_links` 命令）。

### 3.7 profile.json

```json
{ "schemaVersion": 1, "name": "Vanilla+", "active": true,
  "entries": [
    { "modId": "com.author.qol", "version": "1.2.3", "enabled": true,
      "source": "github:owner/repo", "link": "junction" }],
  "configOverrides": ["config/qol.cfg"] }
```

复制 profile 即复制整份清单；`active` 全库最多一个为 true，Apply 成功后写回。

### 3.8 pack.json

```json
{ "schemaVersion": 1, "id": "sample-pack", "name": "Better Vanilla", "version": "1.0.0",
  "payload": "manifest",
  "requires": { "bepinex": "5.4.23.6" },
  "mods": [
    { "modId": "com.author.qol", "version": "1.2.3", "source": "nexus:12345",
      "sha256": "…" }],
  "overrides": ["config/qol.cfg"],
  "createdAt": "2026-11-17T12:00:00Z" }
```

文件就是 zip：`pack.json` + `overrides/**`，`payload=embedded` 时再加 `mods/<modId>/<version>/payload/**`。扩展名 `.cuhubpack`，两种 payload 同一个扩展名（靠字段区分），与 MC 的 manifest / mrpack 同思路。

校验（包是不可信输入）：`schemaVersion` 已知、路径无绝对/`..`/保留名/重复、每个文件对清单 sha256、条目数 ≤4096、单文件 ≤512 MB、总量 ≤2 GB、overrides 只允许落 `config/` 与 `Plugins/` 内指定 mod 目录。

### 3.9 导入与导出

- **导入 manifest**：逐条走既有来源解析（nexus / github / custom）→ 下载 → 入 Store；缺失项列出来，允许「跳过缺失继续」。
- **导入 embedded**：校验后直接入 Store，不联网。
- **导出 manifest**：只写来源与版本，文件几 KB。
- **导出 embedded**：把 Store 里的 payload 与用到的 config 打包，体积 = 内容大小；只作为本地导出、私下传递，Cuhub 不托管、不提供公开包库（AGENTS §3 禁止重托管第三方模组）。

### 3.10 迁移

不自动收编。设置里给一次性「导入现有模组到内容库」：扫描 Plugins → 复制进 Store（先复制、后校验）→ 生成 `Adopted` profile → 逐项确认后把原文件替换成链接（替换前备份到 `Backups/`）。任一项失败就停在那一步，已完成的保留，界面列出待处理项。迁移仍不自动执行；安装/更新已统一走 Store，未迁移的散装模组在首次更新时被收编并备份原文件。

### 3.11 与现有模块的关系

| 模块 | 变化 |
|---|---|
| `install` | 保留 staging、归档布局、`UpgradePlan` 与 BepInEx；单模组安装/更新不再直接写 Plugins，改由 `packages` 落 Store |
| `packages`（新增） | 载荷入库、写活动档案、按档案部署、替换未托管文件前备份；返回 `PackagePlan` / `PackageReport` |
| `hub` | `prepare_package_update` / `apply_package` / `prepare_remote_install` 串起 Store + Profile + Link；旧 `plan_download` / `apply_install` 只留回退 |
| `mods` | 扫描仍读 Plugins（部署结果），快照额外带 `modId` 与来源版本 |
| `backup` | 增「切 profile 前自动备份一次」的可选项，默认关 |
| `modlist`（CUH1） | 保留，转为「轻量清单」；整合包走新格式，不混用 |
| 新增 `store` / `links` / `profiles` / `packs` | 四个新模块，`Hub` 加对应方法 |

### 3.12 破坏性操作

删模组、删版本、清空 Store、替换现有部署、撤销迁移：全部两段式（先 `plan_*` 返回清单，再 `apply`），界面写明作用范围与是否可恢复。

### 3.13 工时（不含 UI 打磨）

| 块 | 工时 |
|---|---|
| Store + Link 内核（含跨卷与权限降级） | 10–14 |
| Profile（Apply/切换/校验） | 4–6 |
| Pack manifest | 6–8 |
| Pack embedded 与导出 | 4 |
| 迁移（Adopt） | 3–4 |
| Multiplayer 页 | 8–12 |

## 4. 交叉：联机时锁 profile

联机中不允许切 profile 或 Apply（对端版本对不上会直接崩会话）。做法：`app` 在 `TunnelStatus=Connected` 或桥报告游戏在跑时禁用切包入口，并提示原因；不写额外的锁文件。

## 5. 里程碑

| 里程碑 | 内容 | 依赖 |
|---|---|---|
| M1 | 联机 MVP：内核获取 → 进程 → 房间码 → hostname 发现 → 双机连通 | 无 |
| M2 | Store + Link 内核 + 迁移 Adopt（无新 UI 也可） | 无 |
| M3 | Profile 与 UI | M2 |
| M4 | Pack manifest 导入导出 | M3 |
| M5 | 桥联动（依赖 mod 作者确认） | M1 |
| M6 | Pack embedded | M4 |

M1 与 M2 可并行，互不依赖。2026-11-17 用户拍板：先 M2 再 M1；当前 M2 已落地，M1 的 core 与联机页已落地，待 Windows 实机与双机联调。

## 6. 安全与合规

- 密钥只在内存与 0600 文件；日志与诊断报告不含码里的密钥段。
- 链接只能指向 Store 内的路径，且 Store 内路径每次写入前重新解析；junction 目标被外部改指向时不跟随操作。
- 整合包每个文件 sha256 校验，任何一条不符即整包拒绝。
- EasyTier 为 LGPL-3.0：分发包附许可证与源码地址，不改二进制。

## 7. 待拍板

1. **顺序**：先 M1 联机 MVP，还是先 M2 内容库内核？
2. **部署策略**：目录型用 junction、单 DLL 用硬链接、异卷退拷贝——接受这个方案吗？
3. **Store 位置**：固定在 appdata，还是允许用户指到游戏盘（跨盘时省一次拷贝）？
4. **迁移**：默认保持旧行为、只在设置里提供「导入现有模组」，可以吗？
5. **包格式**：`.cuhubpack` + `payload` 字段区分 manifest / embedded，可以吗？
6. **embedded 定位**：只做本地导出与私下传递，不做托管与公开包库，确认吗？
7. **联机确认**：房间码 C1 + hostname 发现 + 无广播中继 + 不自提权（要求以管理员重开），确认吗？
8. **联机锁 profile**：连上或游戏在跑时禁止切包，接受吗？
9. **多版本共存**：库内允许同一 mod 多版本并存、但同一 profile 只能启用一份，确认吗？
10. **FEATURES 落地**：批准后新增 Z 段（内容库与整合包）并改写 R 段（Multiplayer），可以吗？

## 8. 参考

- `docs/multiplayer/easytier-plan.md`（联机背景、社区做法、EasyTier 资产与许可）
- `docs/ARCHITECTURE.md`（模块边界、事件与线程模型）
- `docs/core/install/overview.md`、`docs/core/mods/overview.md`（现有安装与扫描语义）
- `docs/FEATURES.md` §3（appdata 布局）、F 段（安装计划）、R 段（Multiplayer 预览）
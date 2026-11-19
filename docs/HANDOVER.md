# 交接说明

截至 2026-11-19。`main` 与 `origin/main` 都在 `1148c4d`，工作副本另有任务中心、批量更新与侧栏折叠的未提交改动（以 `jj st` 为准）；接手前先读本文与 [ARCHITECTURE.md](ARCHITECTURE.md)，再按模块看 [FEATURES.md](FEATURES.md) 与对应 `docs/core/` 说明。

## 1. 仓库现状（最要紧）

- jj 仓库（colocated）。`main` 与 `origin/main` 都在 `1148c4d`；工作副本有任务中心、批量更新与侧栏折叠的未提交改动。
- 2026-11-19 已按 core / app / 桥插件 / docs 拆成 4 个提交并推送：`272f9f4`、`cc8075d`、`9f00143`、`1148c4d`。之后仍按模块原子提交，不要把桥和内容库混在一起。
- 核心模块已从 `mod.rs` 全部改为同级 `<name>.rs`（`steam`、`bridge`、`install`、`easytier`、`platform`），对外路径不变。
- 许可为 AGPL-3.0-or-later。陶瓦（Terracotta）移植代码保留来源与改动说明，台账在 `docs/third-party/terracotta.md`。

## 2. 已完成

### 内容库与档案（M2）

- 默认 Store 在 `<appdata>/Store`，设置向导与设置页都能改；自定义路径就是 Store 根。
- 布局 `<store>/mods/<modId>/<version>/{payload,entry.json}`；导入先写临时目录再 rename，`entry.json` 记 sha256；已有版本拒绝，同版本不同字节由 `packages` 拒绝。
- `links` 维护 `Links.json`（游戏路径 → modId/version/kind/files）；目录优先 junction/symlink，单文件优先硬链接，失败退拷贝；删除只解链或删记录文件，不递归删游戏目录。
- `profiles` 维护 `<appdata>/Profiles/<name>.json` 与 `active.txt`；`plan_apply` / `apply_plan` 只动 Plugins 根层的托管条目。托管条目的启停、删除、改名由档案接管：启停改条目 `enabled` 并重新应用，删除从档案条目移除并应用（Store 副本保留）；只剩 `Links.json` 记录、活动档案里找不到条目的路径仍返回 `ManagedByProfile`。
- `adopt` 把现有散装模组导入 Store、备份原文件到 `Backups/Adopt-*` 再建链接；迁移不自动执行。
- GUI 已有 Profiles 页（新建/应用/删除、从当前部署创建）与 Adopt 卡；Settings 与 Setup 有内容库位置。
- 安装/更新已统一走 `packages`：下载 → `PackagePlan` → Store → 活动档案 → Links；未托管旧目录先备份再收编。安装计划会跳过包内与已装模组同 GUID 的 DLL（F12），避免 CT 整包自带 CUCoreLib 重复。旧 `plan_download` / `apply_install` 只留给 BepInEx 与回退。未做：M4 整合包、`configOverrides` UI、条目级启停/复制、undo adopt、Store 删除入口。

### 任务中心与批量更新（2026-11-19）

- 页面里散落的 pending / 依赖面板与 Settings 的 BepInEx 计划块收进右侧任务中心：右下角常驻按钮 + DANGER 角标（`shell/task_center.rs`），抽屉是 `sheet`（右侧、宽 42%），卡片在 `shell/task_cards.rs`，数据每帧由 `ui/tasks.rs` 现算；抽屉不自动弹出，新任务只让角标脉冲。
- 任务卡五类：安装/更新、依赖、BepInEx、批量、下载。下载读 core 的 `DownloadProgress`（`Ui::watch_downloads` 每 120ms 取快照），缺 Content-Length 时用脉冲条、不编造百分比；失败只写状态行与活动日志，不保留失败卡；游戏运行中被阻断的卡要关游戏后重新点 Apply。
- 批量更新：本地页可更新行的复选框 + 表头全选/清空，选中键是 `source_key` 并在重建行时剪枝；任务中心批量卡提供 Update selected / Update all，点按钮后先逐项预检依赖，任何 Missing / Outdated 都整批不开始并聚合成一张依赖卡；干净批次顺序执行、应用前重查依赖，单项失败只记日志并继续。
- 侧栏折叠改用 NSH 原生布局动画（`shell/sidebar.rs`）：`animate_layout` + `LayoutTransition::spring(Spring::new(220.0).damping_ratio(0.8))`，宽度在 176 / 44 之间一次插值，替换原来的 8 级 × 12ms `set_timeout` 阶梯；连点从当前帧重新收敛，宽窄两套内容仍即时切换。

### 联机（M1）

- `crates/core/src/easytier/`：release（2.6.4 下载/校验/解压）、detect、ports、argument、process（父死子跟、RPC 回环）、status（CLI JSON）、tunnel（房主/客人、客人发现后带 `--port-forward` 重启一次）。
- 房间码自定 C1：16 字符、32 字符表、XOR 校验，显示 `C1-XXXX-XXXX-XXXX-XXXX`；不与陶瓦 Scaffolding 互通。房主 hostname 约定 `cuhub-host-<port>`，客人从 peer list 找房主。
- Multiplayer 页已接：开房/加入、状态卡、房间码复制、对端表、EasyTier 未就绪提示与自定义目录、1500ms 轮询。公共节点默认 4 条；实测 `public.easytier.top`、`public2.easytier.cn`、`etnode.zkitefly.eu.org/node1` 时常握手失败，`/node2` 可用。
- 提权：Windows 要求以管理员重开 Cuhub（`IsUserAnAdmin`，不做 runas 单拉）；Linux/macOS 不预检查 Cuhub 自身能力，由 easytier-core 文件的 `cap_net_admin+ep` 在 exec 时生效。
- 未做：双机同/异 NAT 实测、Windows 实机、延迟/中继列、手动 IP:端口退路、Windows 无边框窗口验证。

### 桥（Bridge，实验性，不发布）

- Rust 宿主：`crates/core/src/bridge/{proto,rendezvous,command,session,handshake,server}.rs` 加 `hub/bridge_host.rs`；监听 127.0.0.1、token、单会话、命令表、三档权限、审计；`crates/core/tests/bridge_server.rs` 13 个回环集成测试。
- C# 游戏侧：`mods/cuhub-bridge/` 的 `CuhubBridge.Core` + `CuhubBridge.Plugin`，`dotnet build -c Release` 通过、无警告；实现发现、hello、10 秒心跳、`mp.state`、`mp.host` / `mp.join`（反射 CUCoreLib），其余命令回 `unavailable`。尚未真机加载。
- 未接线：`take_events` 没有 UI 消费者，Multiplayer 页不显示桥状态；`game.state` / `mods.loaded`、权限确认、`mp.leave` / `mp.kick` / `save.*` / `skin.*` 未做。桥没有打进 Cuhub 安装包，需 `just package-bridge` 生成 zip。
- 详细边界见 [bridge 模块说明](core/bridge/overview.md)、[协议](bridge/protocol.md)、[实施方案](bridge/plan.md)、[C# 研究](bridge/csharp-plugin.md)。

## 3. 当前结构

- core：`paths / settings / network / metadata / hubconfig / nexus_auth / nexus / install / packages / store / links / profiles / adopt / mods / catalogs / modlist / sources / protect / backup / skins / steam / launch / bridge / easytier / room / multiplayer / logs / hub`。
- app：9 项侧栏（Hub Home、Nexus、远程、本地、模组档案、多人游戏、皮肤与备份、设置、关于）；`shell/title_bar.rs` 是无边框 HUD 标题栏，`shell/frame_edges.rs` 是 8 条缩放热区，`ui/fx.rs` 是共用动效时长，`shell/task_center.rs` / `shell/task_cards.rs` 是任务中心与卡片，`ui/tasks.rs` / `ui/batch.rs` 是任务数据与批量 worker；侧栏折叠走原生 `LayoutAnimation`。
- 持久化：`Settings.json`、`Store/`、`Profiles/`、`Links.json`、`Backups/`、`Tools/easytier/`、`multiplayer.json`（0600）；桥的 `bridge.json` 在 Proton prefix 的 `ProgramData/CasualtiesHub/`，`bridge.lock` 在 appdata。
- 内容库自定义根路径修复过：以前会生成 `Store/Store/mods`，现在 Hub 直接把 `Settings::store_dir` 当 Store 根。

## 4. 已拍板 / 约束

- 安装/更新统一走 Store；旧链路仅 BepInEx 与回退。任何回退到直接写 Plugins 的改动都要先问。
- 档案托管条目不做文件级启停/删除；改版本走档案或更新，避免穿过链接改 Store。
- 联机复用陶瓦的进程/房间码思路，但不搬 Scaffolding 协议，也不做 MC 广播中继；游戏侧联机 mod 支持 UDP 直连。
- 桥只监听 loopback，游戏侧输入按不可信处理；CUCoreLib 是软依赖、全反射，不代装；凭据永不进桥。
- 破坏性操作两段式确认；游戏运行时拒绝写游戏目录；任务卡保持阻塞，关游戏后需重新点 Apply。

## 5. 建议下一步

1. 按 [TEST-PLAN.md](TEST-PLAN.md) 跑批次 A，再分环境跑 B–H；当前只有 core 单测与桥回环，UI、真实游戏、双机、Windows 与打包都没有可复现记录。GitHub 源本地清单仍为 2 条；托管仓库 `naroxeno/CUHub-Store` 的远程拉取已接（6h 缓存、启动刷新、本地兜底，Remote Mods 页可手动刷新），清单已上传 26 条并验证 raw 可读（2026-11-19）。
2. 桥的真机验证：`just build-bridge` / `just package-bridge`，把两个 DLL 放进 `BepInEx/plugins/CuhubBridge/`，确认 BepInEx 5.4.23.6 能加载、`mp.state` 有值、`mp.host` / `mp.join` 与游戏内 UI 等价；再决定是否接页面状态。
3. 联机双机实测（同 NAT / 异 NAT），重点看房主发现、`--port-forward` 重启、退出清理；Windows 无边框标题栏也要实机看。
4. 内容库后续：M4 整合包、`configOverrides`、Store 删除入口、undo adopt；先补用户可见入口再动数据格式。
5. 收尾基线警告：`cargo clippy -p cuhub-core --all-targets -- -D warnings` 仍有既有 `hub.rs` 的 `collapsible_if` 与 `settings.rs` 的 `field_reassign_with_default`；与桥无关，修时单独提交。

## 6. 验证命令

```sh
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
cargo fmt --all -- --check
nsh check
cargo test -p cuhub-core          # 301 单测 + 13 桥集成
cargo test -p cuhub --all-targets
cargo clippy -p cuhub --no-deps -- -D warnings

just build-bridge                 # dotnet build，需 dotnet SDK 与游戏 Managed DLL
just package-bridge               # dist/bridge/CuhubBridge.zip
```

## 7. 接手须知

- 版本控制只用 jj：`jj --no-pager st` / `jj diff` / `jj describe` / `jj new`；提交后 `jj bookmark set main -r @`。不要用裸 git 写操作。
- nsh 的 `Input` 是受控组件：`.value(signal)` 只负责显示，必须同时写 `.on_change` 回写信号，否则按钮读到空串。仓库里除已修的档案名输入框外，其它输入都已按这个模式写。
- 默认零注释；解释写 `docs/` 镜像目录。新增/修改行为要同步 FEATURES 对应条目与模块 `overview.md`。
- `tmp-shots/` 是本机截图临时目录，已在 `.git/info/exclude`；`mods/**/bin`、`mods/**/obj` 由 .gitignore 忽略，不要提交构建产物。
- 写盘/删除/解包改动先在一次性目录或临时游戏副本验证，不要拿真实游戏目录试。

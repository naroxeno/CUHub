# CUHub 技术路线图与可行性评估

审查稿 · 2026-11-21

## 建议先做什么

1. **先验收现有联机，再增加联机功能。** 用 R1–R11 验证端口检测、QuickTest 和房间交接，同时记录直连、中继与游戏侧状态。保留手动开房和手动重进路径。
2. **先让模组部署可恢复，再承诺可复现。** Store 和 Profiles 已能作为整合包的基础。当前部署中断后可能留下部分链接、旧档案或未入账的文件变化，先打通故障诊断和恢复，再支持整包应用和联机差异预检。
3. **把 Dedicated 管理与无头研究分开排期。** 本地 Together 源码已有 Dedicated 启动入口，而且 Dedicated 可以在服主无玩家角色时进入并运行 Unity 世界。先做现有模式的启动、状态及测量。低渲染、无图形设备和独立 .NET 分别验收。
4. **先识别运行环境和采集数据，再推荐设置。** 保留 Steam 的兼容工具选择，优先复用外部工具。多人卡顿目前没有实测归因。

本文评估当前工作树。仓库依据为双语 README、[架构](ARCHITECTURE.md)、[功能清单](FEATURES.md)、[v2 设计](DESIGN-v2.md)、[交接](HANDOVER.md)、[测试计划](TEST-PLAN.md)及相关代码与模块文档。游戏侧以本地 `ref/casualties-together`、`ref/CUCoreLib` 和 `ref/upstream` 快照为据，快照行为须与部署 DLL 版本对应。本文未复跑构建、真机或性能实验。接口和优先级供审查，现有架构决策仍以架构文档为准。

## 1. Mod Manager

| 能力 | 当前实现 | 建议与可行性 |
| --- | --- | --- |
| 扫描、安装、开关和更新 | `mods` 读 BepInEx 元数据，`install` 做 staging 和预览，`packages` 接管常规更新与安装。现有来源包括 GitHub、Nexus 和本地归档 | 高。延续现有入口和确认流程，先补失败时的逐项诊断、档案条目编辑和迁移后状态校验 |
| 依赖、冲突和版本约束 | `mods/dependencies.rs` 合并来源和目录的直接依赖，报告缺失与最低版本不足。可选依赖被跳过，无法比较的版本目前不会报过旧。`catalogs` 支持已知冲突查询，安装计划有同 GUID DLL 过滤 | 高可做预检，中等可做自动解析。先用目标档案检查传递依赖、循环、重复 GUID、文件目标碰撞和显式不兼容。将未知版本、缺少候选、约束无解分开。多版本求解等版本索引有足够覆盖再评估 PubGrub，来源只提供 latest 时无解不能解释成版本冲突。线下比较候选 crate 的许可证、维护状态、体积、工具链及 Windows/Linux 支持后再选型。[PubGrub](https://github.com/pubgrub-rs/pubgrub) |
| 事务、回滚和诊断 | 旧链路有替换回滚。新链路先入 Store，再移走需备份的旧路径并逐项应用 Links，最后保存档案。应用失败会保留已完成步骤。索引保存失败也可能已有文件变化。批量更新顺序执行，允许单项失败后继续 | 高但优先。用现有计划与账本做预检、确认及失败定位，评估每个游戏根目录的写入互斥和可恢复记录。区分入库成功与部署提交，执行前验证计划仍适用，按崩溃点测试前滚或恢复原部署。用于恢复的备份要明确所有权和路径，回滚失败应保留证据和可操作报告 |
| Profile、整合包与 Lockfile | Profile 指定 Store 版本，通过 Links 部署启用项。Store 按 `modId/version` 存载荷及逐文件 SHA-256，`CUH1` 是轻量清单。`CUPK1` 与 v2 的 `pack.json` 属于文档中的候选 | 高。先从已部署且重新校验的档案导出只含引用的锁定清单，等恢复验证通过再做可执行导入。锁定精确版本、制品身份和哈希，注明游戏构建、BepInEx、平台或角色条件。缺包、源失效、未授权下载、版本相同但哈希不同均应可诊断。允许用户补交原包，完整重现只在所有必需条目和配置都验证通过时宣告成功 |
| 模组配置 | `configOverrides` 已持久化，档案应用尚未执行。其余游戏文件存在独立保护与备份逻辑 | 中等。先在停游戏时预览和编辑受支持的 BepInEx 配置，保留未知键、编码和注释，支持备份及明确恢复。优先作者提供的类型、范围、是否需重启及同步许可。默认排除密码、本机路径等，分享端只允许明确声明可分发的覆盖项。[BepInEx ConfigFile](https://docs.bepinex.dev/articles/dev_guide/plugin_tutorial/4_configuration.html) |
| 联机环境一致性 | CUHub 有磁盘扫描与 Bridge 握手的已加载 GUID 快照，尚无两端的锁定环境比较。Together 本地快照有游戏文件哈希门槛及模组 GUID 集合检查 | 高可做差异提示，中等可做自动联动。区分期望清单、磁盘部署与当前进程实际加载三份证据。先手动交换锁定清单并比较，列出缺失、不同版本/内容、角色允许差异及未知。后续传递最小化摘要，用户确认后走既有安装流程。Together 严格名单开启时测试仅一端加载 Bridge 会否改变入房结果 |

实现依据见 [packages](core/packages/overview.md)、[profiles](core/profiles/overview.md)、[Store](core/store/overview.md)、[依赖检查](../crates/core/src/mods/dependencies.rs)、[Together 入房判定及 GUID 来源](../ref/casualties-together/ScavPrototypeMP/ScavMultiplayer.cs)。内容库的链接策略包含硬链接、目录链接与拷贝，安装记录中的摘要只能证明导入时的内容。锁定与恢复验收应覆盖已部署载荷被修改、索引漂移、跨卷备份及 Windows 大小写碰撞。对 mod ID、仓库 ID、BepInEx GUID 和同包多个插件之间的对应关系保留来源与冲突证据。

### 从现有安装重建托管档案（MVP）

沿用「模组档案 → 内容库迁移」入口。扫描只读，逐项显示名称、本地版本、识别到的 GitHub 来源、可选动作及跳过原因。匹配沿用 GUID、DLL 名称、显示名的优先级，同级多匹配由玩家选择，仅显示名命中标为低置信。已托管、已禁用和内置 Bridge 条目列出并跳过。`BepInEx/config` 保持原样。

- **本地导入。** 复用现有迁移，把符合条件的原文件作为本地自定义模组收编进 Store，保留原版本与文件内容。沿用缺版本、同版本已入库等现有跳过规则，并显示原因。无 GitHub 候选或离线时仍可走这条路径，迁移全程不调用 Nexus 下载。
- **GitHub 重装。** 逐项选择，通过现有 GitHub 下载、暂存和 `PackagePlan` 更新链路准备。校验包内身份，检查清单和包内声明的依赖。确认前显示本地版本、对应插件的包内版本、旧件整体备份范围和新目标路径。建议允许同版本及升级，降级或版本不可比较时提供本地导入。准备失败不动游戏目录，可改选本地导入。
- **执行和验收。** 游戏关闭后复核已确认的范围，再备份和部署。复用 Store、Profiles 和 Links，逐项显示成功、失败和实际备份位置，单独标明部署成功但档案保存失败。用 release 配置在临时游戏副本验证离线导入、GitHub 同版本与升级、降级拦截、失败保留原件和混合迁移的档案结果。一键批量重装、自动安装依赖和迁移撤销放到后续。

已确认默认导入原文件。玩家逐项选择 GitHub 重装并确认同版本替换或升级。成功项写入同一个新建并激活的 `adopt_*` 档案，保留原档案记录。[迁移规则](core/adopt/overview.md) · [包部署](core/packages/overview.md)

## 2. Network Manager

- **已实现，仍实验性。** EasyTier 固定 2.6.4，已有校验下载、探测、子进程管理、回环管理接口、C1 房间码、房主发现、UDP 转发和对端列表。现有 peer parser 保留 hostname、IPv4、NAT 与本机标志。W4 将游戏端口经 `mp.state` 回读，自动/手填/CUHub 默认 7790 按顺序取值，端口占用显式报错，确认变化后尝试重建并可回到旧端口。客人收到提示后手动重进。Together 文档的直连默认是 **7790**，应在诊断里明确端口的来源。[EasyTier 模块](core/easytier/overview.md) · [端口策略](core/multiplayer/overview.md) · [Together 启动说明](../ref/casualties-together/FOR-NERDS.txt)
- **房间与诊断可增量交付。** 在现有房间状态里记录发现、转发建立、游戏连接、换房和退出阶段。保留当前房间及最近记录即可。对固定版本 CLI 用真实 JSON 样本验证 RTT、丢包、直连/中继、流量字段的含义、时间窗及单位；缺失显示未知。把 EasyTier 对端 RTT 与游戏 RTT/TPS 分开，诊断能定位到工具、发现、转发、游戏四层。先验证手动 IP:端口回退和自管节点，再考虑公开房间发现。[CUHub 状态解析](../crates/core/src/easytier/status.rs) · [EasyTier 2.6.4](https://github.com/EasyTier/EasyTier/tree/v2.6.4)
- **与 Together 的集成优先用上游接口。** CUCoreLib 的 QuickTest Host/Client 对应普通 Host/Client。Together 自有 Steam Lobby、IP 模式，以及按 GUID 集合执行的入房校验。CUCoreLib 有游戏内消息和内容快照 API，可研究候选摘要通道，先核实能在连接前还是只能连接后交换。保留只读差异提示和手动导入路径，避免复制 Lobby、存档或世界状态同步。[CUCoreLib 桥](../ref/CUCoreLib/Networking/MultiplayerBridge.cs) · [CUCoreLib API](../ref/CUCoreLib/Networking/MultiplayerApi.cs) · [Together Steam Lobby](../ref/casualties-together/ScavPrototypeMP/Net/SteamworksNet.cs)
- **服务端集成可行。** 由 Network 管理隧道、节点及转发，Server 管理游戏实例。先手动配置私有节点，验证权限、故障与停服清理，凭据和房间秘密留在本地。常驻服务和公网运营另行评估。

上一会话报告通过 377 个 core 单测、14 个集成测试、25 个 app 测试，Bridge 独立端口隔离及关闭竞态也有自动回归记录。该记录和源码支持实验性实现已接线。两机 R1–R11 仍待执行，端口反射成功路径使用替身 DLL 验证，QuickTest 是否在正式版实机可用仍待确认。已有单机观察与完整双机验收应分别记账。[交接](HANDOVER.md) · [回归记录](bridge/w4-regression.md) · [工单与裁定](bridge/w4-tickets/README.md)

## 3. Runtime Manager

| 阶段 | 建议 | 可行性与依赖 |
| --- | --- | --- |
| 只读发现与启动诊断 | 复用 `steam` 库扫描、AppID 反查和 `launch` 的 URI/命令回退。记录游戏 build、实际进程、架构和 BepInEx 是否加载。覆盖 Windows 原生、Linux 原生 Steam 与 Flatpak Steam、自定义库和多 GPU | 高。当前启动返回 AppID 表示交给 Steam，需要独立证据确认游戏已启动。正式版 4576490 与 Demo 4576510 分开记账。本地上游文档中的 Demo 测试流程不能替代正式版验收。[启动实现](../crates/core/src/launch.rs) · [正式版](https://store.steampowered.com/app/4576490/) · [Demo](https://store.steampowered.com/app/4576510/) |
| 硬件能力 | 读 CPU、GPU、驱动、实际 Vulkan 设备/API/扩展，报告宿主探针与 Steam 容器或游戏内探针的差异。Windows 原生以 DX11 为基线，Linux 按已选 DXVK 版本要求判断 | 高。可选用 Khronos Vulkan-Tools 的 `vulkaninfo`，Windows DXGI 等能力探测按 OS 收口。区分未安装探针、驱动不可用、软件渲染及设备未实际选中。以诊断报告作为首个交付。[Vulkan-Tools](https://github.com/KhronosGroup/Vulkan-Tools) · [DXVK](https://github.com/doitsujin/dxvk) |
| Proton 与 DXVK 选择 | Steam 管官方 Proton。只读列举已装工具，先用 Steam 手工选择与 ProtonUp-Qt 等外部工具安装 GE-Proton。DWProton 可进入对照组，验证来源、校验材料和支持矩阵。DXVK 优先跟随所选 Proton 构建，独立版本及变体仅在隔离的可恢复环境对照 | 中等。现有 Steam 启动委托无环境选择语义。用官方 Launch Options 做显式控制，验证参数转发、前缀迁移、BepInEx 和 Steam Lobby，回退到既有设置。WineD3D 可作诊断对照。维护和分发由上游承担，CUHub 负责选型、诊断和可撤销配置。[Proton](https://github.com/ValveSoftware/Proton) · [GE-Proton](https://github.com/GloriousEggroll/proton-ge-custom) · [DWProton](https://dawn.wine/dawn-winery/dwproton.git) · [ProtonUp-Qt](https://github.com/DavidoTek/ProtonUp-Qt) |
| Benchmark 与推荐 | Linux 复用 MangoHud 日志，Windows 复用 PresentMon。先能导出本地样本，再做基线及运行参数 A/B。规则用有限字段表达游戏 build、OS/GPU/驱动、运行时约束、可选设置、证据和恢复动作 | 采集高，自动推荐中等且依赖样本。仅在规则与实测指纹匹配时展示理由、样本条件及回退。多条规则候选时明确优先级并预览，未知硬件使用默认配置。保持社区提交实测数据自愿，由用户导出。[MangoHud](https://github.com/flightlessmango/MangoHud) · [PresentMon](https://github.com/GameTechDev/PresentMon) |

## 4. Server Manager

**本地源码已回答世界运行位置的一部分。** `Plugin.cs` 处理 `--mp-startserver` 并进入 Dedicated 角色。 `ServerMain.FixedUpdate` 统计 TPS，Dedicated 分支调用游戏本体的 Body 和刚体接口。AutoContinue 在满足条件时启用进局流程，`ScavMultiplayer.cs` 在 Dedicated 世界场景启动旁观模式。世界生成 Harmony 补丁、`WorldMP` 及网络实体管理表明 Dedicated Unity 实例承担世界运行及至少部分服务端状态，客户端也有同步与实体处理路径。玩家移动、物理、物品、世界区块各自的权威和预测边界仍需逐条核对。主菜单可等待玩家只是运行周期的一种状态。[启动入口](../ref/casualties-together/ScavPrototypeMP/Plugin.cs) · [服务端循环](../ref/casualties-together/ScavPrototypeMP/ServerMain.cs) · [世界生成补丁](../ref/casualties-together/ScavPrototypeMP/Patches/WorldGenerationPatches.cs) · [同步实体](../ref/casualties-together/ScavPrototypeMP/Net/ScavLiteEntitySystem.cs)

| 候选 | 结论与条件 |
| --- | --- |
| 管理现有 Dedicated | 高可行，先做启动预览、规则值校验、状态与日志。上游已有启动参数、AutoContinue、最少玩家阈值与规则预设。首版用结构化参数和已知规则，敏感值避开命令行、导出和日志。先验证 Steam 启动委托如何传参，区分请求已提交、进程已观察到、开始监听、可进房、世界加载、退出。停止优先使用有证据的上游方式，异常重启设有限退避，验证身份后才管理进程，防止误停普通客户端。[上游说明](../ref/casualties-together/FOR-NERDS.txt) |
| EasyTier 与资源监控 | 高可行。先用专用测试实例，按角色记录 PID 生命周期、进程 CPU、内存、GPU 指标和网络量。用 Bridge 读世界阶段、版本、TPS 与人数；渲染帧率、Unity FixedUpdate 计数和有效网络更新节奏单独呈现。工具丢失或桥断开可继续展示 OS 证据 |
| 实验性低渲染 | 中等。先比较上游小窗口示例、标准窗口、可逆的画质/帧率设置。源码的 `DedicatedServerDisableVisuals` 声明和语音视觉相关分支只支持局部效果假设。不要将限帧视为天然安全，Unity 主线程、发包和更新节奏都应回归。禁用相机、后处理或音频前先核实初始化、场景加载和世界协程。[现有视觉开关及场景逻辑](../ref/casualties-together/ScavPrototypeMP/ScavMultiplayer.cs) · [语音实现](../ref/casualties-together/ScavPrototypeMP/Content/Voicechat.cs) |
| Unity 无图形设备模式 | 研究。有已发行 Player 的 batch/no-graphics 实验，也有从 Unity 项目构建 Dedicated Server target 的途径。前者依赖游戏版本、Steam/BepInEx 注入、首场景、Shader/纹理、音频等初始化及切层是否可用。后者依赖原项目和构建授权，适合游戏作者合作。[Player 桌面无头](https://docs.unity.com/en-us/engine/6000.3/manual/platform-specific/dedicated-server/desktop-headless-mode) · [Dedicated Server 构建](https://docs.unity.com/en-us/engine/6000.0/manual/platform-specific/dedicated-server/build) |
| 独立 .NET | 高风险研究。Together 的 `netstandard2.1` 项目仍引用游戏 Assembly-CSharp 和 Unity 模块，实际路径依赖 MonoBehaviour、协程、场景、游戏世界和物理。可先盘点纯传输、序列化与规则代码能否单独测试。独立运行世界需要再现引擎与玩法语义、验证跨版本一致性并取得必要授权。当前只建议做依赖图和合作可行性判断。[Together 项目引用](../ref/casualties-together/ScavPrototypeMP/CasualtiesTogether.csproj) |

Together 本地源码已有 `Optimizinator` 中的专项探针、`Extras/SendBudget.cs` 及 FPS/TPS 读数。先验证能否复用，避免重复打补丁。游戏和存档生命周期依赖上游，CUHub 只消费已公开的启动、状态及诊断能力。[优化与探针目录](../ref/casualties-together/ScavPrototypeMP/Optimizinator) · [发送预算](../ref/casualties-together/ScavPrototypeMP/Extras/SendBudget.cs)

### 多人性能实验

- 每轮固定正式版 build、Together/CUCoreLib/BepInEx 版本、运行时、图形设置、同一合法测试世界与操作脚本。记录人数口径，分别注明有无本地玩家角色。测 1/2/3/4 人，4 人先验证可复现，再增加负载。菜单等待、静止、分散探索、世界切换及语音活动分别测。记录冷启动/预热，候选与基线交替重复，报告运行间波动。
- 同时采集服端和客户端 P50/P95/P99 帧时间、进程/核心 CPU、GPU busy、内存/GC、固定更新与同步耗时、消息数/流量/重传、RTT/丢包及直连或中继状态。缺少计数器时注明。统一场景标记和时间基准，记录采样开销。
- 先比较普通 Host 和 Dedicated，再在同一模式逐项改变直连/中继、画质和渲染设置。GPU 指标改善但服务器 tick 和客户端延迟不变时，收益仅记为渲染资源节省。主线程、GC、世界模拟、同步预算、网络和客户端渲染都保留为假设，要求数据支持才归因。

## 5. CUHub Bridge

现有 C# 插件已经能建立回环 IPC，在 Unity `Update` 泵处理命令。Rust 宿主有帧、握手、单会话、心跳和命令权限。已接游戏侧 `mp.state/host/join`，握手携带一份加载 GUID 快照。持续 `mods.loaded`、`game.state`、性能流、Dedicated 生命周期以及完整权限确认仍属待实现。共享常量 token 是防误连机制，自动 Dedicated 管理尤其要验证实例归属，现有固定回环端口与单会话约束适合首版只观察一个实例。[桥模块](core/bridge/overview.md) · [C# 主循环](../mods/cuhub-bridge/CuhubBridge.Plugin/BridgePlugin.cs) · [W4 后端](../mods/cuhub-bridge/CuhubBridge.Plugin/MpBackend.cs)

- **先读状态。** 通过版本化能力与有限长快照返回游戏阶段、实际加载模组身份、角色及监听端口，提供来源、时间戳、新鲜度和缺失原因。对不同 Together/CUCoreLib 版本做低成本探针，优先公开 API，必要反射按版本约束。端口读数与游戏入房仍需真机校验。
- **再加本地性能采集。** 固定上限的缓冲与采样窗口，主线程只读 Unity 对象和计数器，聚合、编码、写盘放到已有后台边界。先探测正式 Player 可用的 `ProfilerRecorder` 计数器；单独记录渲染、主线程、GC、模拟和网络数据的可用性。Bridge 关闭、采样关闭及开启三组测自身开销。[ProfilerRecorder](https://docs.unity.com/en-us/engine/6000.6/script-reference/unity/profiling/profilerrecorder)
- **专项调优最后接入。** 先复用游戏设置和上游探针，只有基线数据支持才研究精确的 Harmony patch。绑定游戏版本和功能探针，随时停用并恢复，有逻辑、同步、进局或重连回归就撤回。控制只走声明过的受限命令，凭据、本地配置和原始诊断默认留本机。[Harmony patching](https://harmony.pardeike.net/v2/articles/patching.html)

## 6. 开放模组规范 RFC

建议先发布推荐性 RFC 草案，联同已有模组作者试填。规范使用游戏社区可独立实施的示例、版本化 Schema 和一致性样本。CUHub 的 Profile、Store 和 Links 作为本地实现。旧 ZIP、7z、RAR、裸 DLL 仍可导入，缺失信息可由作者声明、BepInEx 元数据和人工映射逐步补足，保留各字段来源。

| 参考 | 借鉴点 | CU 需要决定的语义 |
| --- | --- | --- |
| [Thunderstore 包规范](https://wiki.thunderstore.io/mods/creating-a-package)与[目录约定](https://wiki.thunderstore.io/mods/packaging-your-mods) | 根清单、作者/项目命名空间、可机器读取的版本及依赖 | 接受 BepInEx guid 与平台身份作为别名，定义独立稳定包 ID、命名空间归属、转移及别名冲突。包与插件可一对多 |
| [Modrinth mrpack](https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack) | 文件级哈希、来源、双方环境条件及 overrides | 先支持引用式整合包，区分包哈希和标准化部署内容哈希，明确路径与跨平台大小写规则。覆盖内容限允许目录，嵌入内容逐项核验再分发权 |
| [Cargo Manifest 和 Lockfile](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html) | 意图声明与解析结果各自有生命周期 | Manifest 声明兼容范围、必需/可选依赖、冲突、游戏构建与加载器要求。Lockfile 精确锁定直接及传递制品、来源和哈希。讨论 SemVer 与旧版号映射，旧版本可保留为不透明标签 |

RFC 首版应另外定义客户端/服务端 required、optional、unsupported 与 unknown 的语义，明确哪些模组和配置要参加联机一致性检查。声明许可证与分发来源，哈希用于完整性验证，来源信任与再分发许可独立确认。Nexus 依照授权 API 或浏览器路径获取，锁文件保留可追溯的制品标识。配置只推荐可移植且经许可的覆盖项。

开发指南优先引用 BepInEx、Harmony 和 CUCoreLib。建议覆盖稳定 GUID、声明依赖、按游戏 build 验证 patch 目标、主线程约束、角色与载荷校验、可选依赖降级、日志与版本排障。模板使用开发者本地合法安装中的游戏引用，产物排除游戏程序集。顺序为 RFC 样例与测试包 → Schema 与离线验证 → 打包 CLI → C# 模板及 GitHub Actions 工作流。等社区采用证据后扩大范围。[CUCoreLib 开发文档](https://cucorelib.web.app/) · [本地 CUCoreLib 项目](../ref/CUCoreLib/CUCoreLib.csproj)

## 7. 模块边界与候选接口

现状有两处文档差异会影响后续实现。架构表把 CUPK1 列在已有模块职责中，功能清单 U11/Z 与当前实现仍把它留在待做；架构的 macOS 旧备注也早于目前的 `platform/macos.rs`，平台文件存在只说明有平台入口，游戏本身仍按 Windows 与 Linux/Proton 的验收范围规划。C# 上游参照中，`GameInstallDetector`、`SteamLibraryLocator`、`ModService` 等是现有 Rust 设计的对照，相关能力优先从现有 Rust 服务扩展。[原启动器服务目录](../ref/upstream/Casualties%20Hub/Services/) · [平台实现](../crates/core/src/platform/macos.rs)

保留 `app → Hub → core 服务` 与独立 C# 插件边界。优先在现有 crate 内落地，通过返回值或快照提供状态，沿用 worker 及现有事件规则。下列操作名用于描述输入与输出。

| 所有权 | 候选入口与输出 |
| --- | --- |
| Mod 管理 | `plan_profile_change` 输入目标档案和当前部署快照，返回依赖、文件冲突、计划及证据。确认后 `apply_profile_change` 返回实际操作、恢复状态与失败范围。 `compare_environment` 是纯数据比较，可同时供本地和联机使用 |
| Network | `room_status` 返回房间阶段、路由、转发及当前网络采样。消费可选的游戏监听快照和只读环境摘要。模组下载安装由用户经 Mod 服务确认 |
| Runtime | `probe_runtime` 返回平台能力和实际运行证据，`prepare_launch` 返回 Steam 启动意图与受支持选项。系统探针放入平台边界。保留交给 Steam 与游戏确认就绪的区分 |
| Server | 输入已确认的角色、规则、Runtime 启动意图和可选 Network 绑定，返回明确实例身份、状态和资源快照。Bridge 补充游戏内状态。Steam 委托与上游手动启动也可纳入观察 |
| Bridge | 回环、限量、按能力取只读快照或调用明确命令。独立于 Steam、Store 和隧道。跨主机交换走另行审查的游戏或网络通道 |
| 规范与工具链 | Schema、示例及验证规则可被独立工具使用，与 Rust 类型、CLI 参数及桥协议各自演进 |

安装、覆盖或恢复应继续两段式确认，游戏运行时保持写保护。先复用现有日志和诊断路径。相关流程现阶段仅在一次性目录、合法游戏副本或双方同意的真机环境验证。长期常驻守护、多实例 IPC、公共房间目录和远程管理各需使用场景及单独设计审查。

## 8. 开发阶段

| 阶段 | 范围 | 进入下一阶段的证据 |
| --- | --- | --- |
| P0 当前基线 | 完成 R1–R11、Windows 与 Linux/Proton 基础启动、诊断记账。建立 Store/Links/Profiles 故障矩阵。确认社区最常见的装包和联机失败 | 双机记录包含手动退路、端口来源及异常恢复。能复现部署失败并判断当前状态 |
| P1 可靠性与观测 | 可恢复部署及故障报告、依赖和冲突预检、档案编辑、从现有安装重建托管档案 MVP；房间诊断；只读运行时探针；单实例 Dedicated 观测与启动验证 | 两平台临时目录通过失效恢复，离线导入与逐项 GitHub 重装通过迁移验收，能定位联机失败层，确认正式版启动及 Ready 条件，首次多人测量可重复 |
| P2 可复现环境 | 引用式锁文件和整合包、配置预览与安全应用、主客差异提示。与社区并行审查 RFC。交付可选的本地 Benchmark 和可回退设置 | 两份干净安装能按授权来源重建并给出相同必需制品摘要。缺失资产有手动路径。角色差异和 Together 现有名单规则经过联机回归 |
| P3 证据驱动扩展 | 视使用反馈交付私有节点与 Dedicated 自动管理、规则推荐、Schema/CLI 和模板。渲染削减、Unity 无头和独立 .NET 研究分别验收 | 有实际用户需求、可复现实验与清晰回退。独立服务端如涉及本体项目或分发，需先有作者协作及授权路径 |

## 9. Research Tasks

| 编号 | 要验证的问题 | 交付证据与决策 |
| --- | --- | --- |
| R-MOD-1 | 真实包能否提供可靠的身份与多版本元数据 | 用经授权的代表性单 DLL、复合包、旧版号和同 GUID 包做样本表。比较手动预检与现成求解器需要的数据，决定自动求解范围 |
| R-MOD-2 | 部署在各写入阶段中断后能否恢复 | 临时目录注入备份/解链/部署/索引/档案失败，另测强退、重试、并发、跨卷、链接与拷贝。每次记录部署、索引、活动档案及残留，验收可重跑且不损失用户数据 |
| R-MOD-3 | Lockfile 与配置能否跨两台机器重现 | 干净环境分别装同一清单，测来源消失、同版本异载荷、手工补包、配置差异和角色差异，给出完整、部分、不可验证的不同结果 |
| R-NET-1 | W4 与 EasyTier 的实际兼容矩阵 | 双机执行 R1–R11 和同/异 NAT、直连/中继、Windows/Linux；保存脱敏的 CLI 样本、游戏状态与时间线。验证端口漂移和反射缺省时的手动退路 |
| R-NET-2 | Together 的入房校验与 Bridge 能否共存 | 固定部署 DLL，分别验证 IP/Steam、名单开关、仅一端加载 Bridge、客户端可选模组，以及何时能交换摘要。记录上游行为与 CUHub 预检结果 |
| R-RT-1 | 正式版兼容工具、启动选项与实际进程归属 | 对比 Windows、Linux 原生/Flatpak Steam、自定义库和多 GPU，记录选用运行时、BepInEx 加载、前缀变化、Steam Lobby 和恢复路径 |
| R-BR-1 | 零售 Player 可观测哪些信号 | 确认实际 Unity 版本和可用计数器，交付采样的功能、耗时和 GC 数据。比对外部采集，断线与不支持信号应有独立状态 |
| R-SRV-1 | Dedicated 服务生命周期与各系统权威 | 用固定 Together 快照及对应 DLL，追踪启动、进世界、切层、重连、退出，并画出客户端与服务端对移动、物品、区块和保存的读写边界。用已知上游规则验证是否能稳定启动并观察 Ready |
| R-SRV-2 | 四人卡顿出现在哪条路径 | 按第 4 节采集对照矩阵与原始本地数据，报告复现率、指标变化及无法归因的变量。低渲染实验只改变一个变量，至少验证进房、进世界、切层与重连 |
| R-SRV-3 | 已发行 Player 是否能在无图形设备下稳定运行 | 隔离副本验证启动、插件、联网、生成世界、多人跑图、切层、退出，和普通 Dedicated 比较模拟与资源。失败记录首个引擎或游戏依赖，再判断作者合作的 Server target 是否值得做 |
| R-SRV-4 | 能否提取独立且有用的纯 .NET 子集 | 产出依赖图、最小纯组件测试、版本维护成本和许可证边界。只有上游协作且能证实世界语义等价时才讨论独立服务端 |
| R-RFC-1 | 社区能否填报并消费规范 | 邀请至少两位模组作者覆盖不同包形态，和一个整合包维护者试用同一 Schema 草案。收集字段缺失、许可、命名权以及旧包导入问题，决定最小必需字段 |

首轮人工审查建议确认三个决策。第一，P0 对正式版 Windows 与 Proton 的实机验收矩阵。第二，P1 部署失败的目标恢复语义及持久化影响。第三，P2 是否先以引用式锁文件和手工联机比对试点。涉及对外接口、格式、依赖、持久化或安全边界的实施，应提交独立短方案由开发者确认。

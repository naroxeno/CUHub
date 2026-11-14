# Casualties Hub 功能清单（Rust + nsh 重写用）

上游参照：`Casualties-Hub/Casualties-Hub-Public-Releases`（C# / .NET 10 / Avalonia 11），源码快照 0.0.8-pre.6.1，公开版本 0.0.9-pre.2。
本文只做**功能盘点**：把现有应用拆成可独立实现、可验收的条目，并标注优先级与 Rust/nsh 侧的落点。
标记：**P0** 核心可用（没有它产品不成立）／**P1** 完整度（正式版应有）／**P2** 打磨与彩蛋／**⚠** 需要先用 nsh 做能力验证。

---

## 0. 产品定位

Casualties Unknown（Nexus 游戏域 `scavprototype`）的 BepInEx 模组管理器 + 启动器，单机桌面应用：

- 浏览 Nexus 社区模组目录（元数据来自社区维护的 nexusmods.json，不是抓 Nexus 网页）；
- 把下载好的压缩包装进 `BepInEx/Plugins`，管理启用/禁用/删除/更新提示；
- 保护用户自己改过的皮肤与资源，重装后可还原；
- 用分享码交换 modlist；
- 预览 CustomSprites 皮肤（按游戏内骨骼摆位合成）；
- 通过 Steam 启动游戏（Proton 下必须走 steam:// 否则 BepInEx 不加载）；
- 公告、发布说明、诊断日志、像素风界面。

**硬性约束（必须继承，属产品规则而非技术细节）**

1. 不绕过 Nexus 的认证 / Premium / 下载页 / 速率限制 / 广告；不抓取 Nexus；不伪造或复用抓包得到的下载链接；不冒充 Vortex；不重托管模组文件。
2. 普通用户（无 Premium key）一律回到原始 Nexus 页面；只有用户自己的 Premium key 才走官方 API 直连下载。
3. Nexus key 不进日志、不进远端请求、不提交仓库；只存本机。
4. 全部远端响应视为不可信输入（描述、公告、链接、元数据）；链接只允许 http/https/steam。
5. 不做遥测：不建安装 ID、不上报活跃度（0.0.8 起已移除 Supabase 与匿名 UUID）。
6. 用户数据（设置、日志、备份、受保护资源、下载、凭据）全部放在仓库之外的应用数据目录。
7. 破坏性操作（删全部模组、覆盖文件、还原备份）必须显式确认，并写清作用范围。

---

## 1. 功能域总览

| 域 | 内容 | 优先级 |
|---|---|---|
| A 应用外壳 | 侧栏导航、顶栏、状态栏、窗口、字体缩放、开发者控制台入口、彩蛋 | P0/P1 |
| B 游戏目录识别 | 手动选择、自动检测（Steam 库扫描）、三种路径粒度 | P0 |
| C Nexus Dashboard | 社区目录列表、搜索/过滤/排序、18+、双视图、详情、右键菜单、下载入口 | P0 |
| D 下载收件箱 | 目录监视、下载完成判定、自动导入询问、归档移动 | P0 |
| E 本地模组管理 | 扫描、版本读取、更新判定、启用/禁用、删除、批量 | P0 |
| F 压缩包安装 | 分类、安装计划、覆盖警告、皮肤槽位、依赖提示 | P0 |
| G 依赖/兼容/已知 bug | 目录数据、缺失依赖、冲突、已知 bug、忽略 | P1 |
| H 分享码 | 生成、导入、缺失项处理 | P1 |
| I 受保护资源 | 保护文件/文件夹、全部还原、移除 | P1 |
| J 备份 | 创建、还原、删除、打开 | P1 |
| K 皮肤预览 | 槽位发现、骨骼合成、头部/表情、镜像、缩放平移、缺失提示 | P1 |
| L Hub Home | 公告、历史公告、What Changed、发布信息、外链、Credits | P1 |
| M Nexus 凭据与直连下载 | key 保存/加密、files.json 选文件、download_link、落盘 | P1 |
| N Steam 启动 | app id 解析、steam:// 启动、回退 | P0 |
| O 外观系统 | 像素风固定调色板、1px 双框、像素字体、整数缩放、语义状态色 | P1 |
| P 设置与迁移 | 全部设置项、旧配置迁移、JSON 容错 | P0 |
| Q 日志/诊断/崩溃报告 | 会话日志、保留策略、崩溃报告、诊断命令、自检命令 | P0 |
| R 多人页（预览） | 样例数据、过滤、布局占位 | P2 |
| S 打包发布 | 单文件、内嵌资源、Data 覆盖、双平台、版本注入 | P0 |
| T 已移除功能 | 自更新、应用内卸载器、HubContent 订阅源、Supabase | 不复刻 |
| U 来源表与整合包 | 来源优先级、GitHub 内置 tag、用户正则、表校验、CUPK1 整合包 | P2 |

---

## 2. 逐项功能

### A. 应用外壳与导航

- **A1 (P0)** 左侧栏 6 个入口：Nexus Dashboard / Local Mods / Multiplayer / Skins & Backups / Hub Home / Settings；Credits 不进侧栏，由 Hub Home 打开。每次切页重建页面实例（读最新设置，等价于“切页即刷新”）。
- **A2 (P0)** 顶栏：Launch Game（主操作，始终可见）、Report issues（URL 来自 Hub 配置；为空则禁用并 tooltip “Link not available.”，否则 “Report a bug or issue”）。
- **A3 (P0)** 底部状态栏：单一 status 文本，由各页面与外壳写入（“Game folder detected.”、“Asked Steam to launch Casualties Unknown.”、“Could not launch: …” 等）。
- **A4 (P1)** 侧栏底部显示 `v{version}`；3 秒内点击 5 次打开开发者控制台（Debug 页/窗口）。这是隐藏入口，不走设置。
- **A5 (P2)** 彩蛋：需在设置中开启 Easter Eggs 才生效（默认关，首次启动写一次 Initialized 标记，避免“默认开”误判）；含一段固定外链跳转与吉祥物点击计数。
- **A6 (P1)** 窗口：Linux 用客户端装饰 + 图标；像素壳（纯黑底、1px 白描边双框、方角导航条、无圆角无阴影），规格见 O。
- **A7 (P0)** 全局缩放：1x/2x/3x（默认 2x）整数倍，作用到整个窗口（继承），启动时应用、改设置即时应用；不提供任意字号。
- **A8 (P1)** 启动流程：读设置 → 应用界面缩放（1x/2x/3x）→ 装载缓存公告并渲染 → 公告过期则后台刷新 → 启动下载监视器 → 游戏路径为空则自动检测（≤20s，失败弹检测对话框可跳设置）。
- **A9 (P1)** 单实例行为：当前实现无单实例锁（不实现也可），但下载监视器必须只跑一份。

### B. 游戏目录识别

- **B1 (P0)** 设置里可选三种粒度：游戏根 / `BepInEx` / `BepInEx/Plugins`；内部统一解析出 Plugins 与应用根（Proton 常建小写 `plugins`，必须逐段大小写不敏感解析，但返回真实大小写路径）。
- **B2 (P0)** 自动检测：Steam 库清单 → `steamapps/common/*` 中含 “Casualties” 的目录；超时/失败返回空（不阻塞启动）。
- **B3 (P1)** 检测对话框：说明“不知道游戏在哪就无法判断已安装什么”，提供 Detect automatically / Choose folder… / Open Settings；检测成功回写设置并提示。
- **B4 (P1)** 路径安全：所有拼接都走“解析真实子项 + is_inside 校验”；比较规则 Windows 大小写不敏感、Linux 敏感（否则 Linux 上会把不同文件当同一个，涉及删除时是数据事故）。
- **B5 (P1)** 下载目录默认值：读 XDG `user-dirs.dirs` 的 `XDG_DOWNLOAD_DIR`，成功解析且存在才用，否则 `~/Downloads`。

### C. Nexus Dashboard

- **C1 (P0)** 数据源：社区元数据 `nexusmods.json`（GitHub raw），整个列表一次取回，本地分页 50 条/页；页面底部 “Page X of Y · N mods”。
- **C2 (P0)** 缓存策略：成功列表缓存 6 小时（成人标记会变，所以不能久）；请求带 `If-None-Match`/`If-Modified-Since`，304 复用缓存；网络失败退回离线缓存；两者皆无时显示错误态但不阻塞本地模组管理。
- **C3 (P0)** 搜索：匹配 名称 / 作者 / 描述（描述先做 BBCode 去标记再匹配）。
- **C4 (P0)** 过滤下拉：All mods / Installed / Not installed / Updates available。
- **C5 (P0)** 排序下拉：Most downloaded / Most endorsed / Name A-Z / Author A-Z。
- **C6 (P0)** 18+ 复选：默认关；关时不显示 `ContainsAdultContent`；tooltip 说明。
- **C7 (P0)** 双视图：tiles（封面 + 底部名称/作者/下载数）与 list（小缩略图 + 名称 + 作者 + 单行摘要 + 下载数 + 认可数）；切换按钮为分段控件，选择跨启动记忆。
- **C8 (P0)** 详情：点 tile → 网格上浮层（完整描述、图、统计、动作、依赖行），X / 再点同 tile / 点外部关闭，网格保持滚动位置；list 视图点行 → 行下展开描述与大图。
- **C9 (P0)** 右键菜单：模组动作（Download/Open Download/Update/…）、显示/隐藏详情、View mod page on Nexus、Copy Nexus link。
- **C10 (P0)** 状态徽标：Installed / Update available / Disabled，用文字色区分（accent / warn / danger）+ 卡片左缘 2px 同色竖条，不铺底色。
- **C11 (P0)** 封面图：异步抓取 + 磁盘缓存（按 URL 的 SHA-256 截断命名），并发上限 4，失败记一次本会话不再重试，只允许 http/https，解码在工作线程，缺失时显示 “No image”。
- **C12 (P1)** 描述渲染：去 BBCode（`[url=]…[/url]` 留文字、`[br]`/`<br>` 归一为换行、其余标签剥除、HTML 实体解码、3 行以上空行压成 1 行）；仅在作者留空行处出现段间距（单倍行距）。
- **C13 (P1)** 动作语义：无 Premium key → “Open Download” 打开 Nexus Files 页（`?tab=files`）；有 key → “Download” 走直连；本地副本过期 → “Open Update”；本地已禁用 → “Re-enable in Local Mods”；模组被来源表指到 GitHub／自定义页时，按 U 的规则换成 “Open release” / “Open source page”（见 U1）。
- **C14 (P1)** 外链按钮（↗）：打开普通模组页；复制链接进剪贴板。
- **C15 (P1)** 依赖行：来自依赖目录，“Requires: …”，无数据时 “No known dependencies”。
- **C16 (P1)** 无游戏目录态：整页替换为空态卡片（No game folder set + 三个按钮）。
- **C17 (P2)** 统计展示：下载数、唯一下载数、认可数（含 hover tooltip 全量数字），紧凑格式。
- **C18 (P2)** 描述为空的兜底文案：“No description was supplied by the mod author.”。
- **C19 (P1)** 刷新按钮：强制重取目录（忽略缓存），期间置 loading 态。

> 上游还有一个 Nexus GraphQL “popular mods” 客户端（`NexusService`，POST `api.nexusmods.com/v2/graphql`，取前 50），当前 Dashboard 实际走社区目录。重写建议：只在需要官方数据时保留，默认不实现。

### D. 下载收件箱与自动导入

- **D1 (P0)** 监视下载目录（可配置）中的 `.zip/.7z/.rar`；监听 Created + Renamed（浏览器先写 `.part` 再改名），忽略 `.part/.crdownload/.download/.tmp/.partial/.opdownload` 与 dotfile；同一路径只处理一次（并发字典去重）。
- **D2 (P0)** 完成判定双策略：Windows = 能以独占方式打开且长度 > 0（2s 间隔，最多 30 次）；Linux = 大小连续 3 次不变（1s 间隔，最多 60 次，注意 Linux 的锁是 advisory，独占打开会在下载中成功）。超时/文件消失 → 记日志放弃，不弹窗。
- **D3 (P0)** 判定完成且已配置 Plugins 时，弹询问对话框（Yes/No）；No 不动文件，Yes 进入安装流程。
- **D4 (P0)** 安装前先 InspectArchive 出计划，再按计划安装；安装成功事件驱动本地模组页刷新。
- **D5 (P0)** 装完默认把原压缩包移出收件箱到 `<appdata>/ImportedDownloads/yyyy-MM-dd_HH-mm-ss_<name>`，避免反复导入；设置“Keep imported archives in the downloads folder”为真则原地保留。
- **D6 (P1)** 移动失败不影响“安装已成功”的结论，只记日志。
- **D7 (P1)** 未配置 Plugins 时不询问，只记日志。
- **D8 (P1)** 监视器启动失败给出可操作提示（Linux 提示 `fs.inotify.max_user_watches`）。
- **D9 (P1)** 皮肤包：完成判定后先问目标槽位（st0–st9 显示占用情况），选定后安装。
- **D10 (P2)** 用户关闭对话框/取消 → 归档留在原地，不再自动重试。

### E. 本地模组管理

- **E1 (P0)** 扫描 `Plugins` 下一级：目录型模组、`.dll`，以及禁用态 `.dll.disabled`；按名称排序。
- **E2 (P0)** 每项信息：名称、安装版本、期望版本、Assembly 版本、BepInEx 插件版本、文件修改时间、GUID、Nexus URL、DLL 路径列表、来源路径、是否禁用、是否过期、缺失依赖、冲突、已知 bug、是否被分享码请求。
- **E3 (P0)** 版本证据展示：“Assembly x | BepInEx y | Modified t”，没有证据时 “No version evidence found”。
- **E4 (P0)** 更新判定（与元数据比对）：0.0.0.0 视为占位符不算过期；只要任一新于元数据即 “up to date”；文件修改时间 ≥ 元数据 `LatestFileModifiedUtc` 也算 up to date；全部新于全部期望值才算 out of date。被禁用的模组不报更新。
- **E5 (P0)** 状态文案（逐字复刻）：Disabled — re-enable… / placeholder 提示 / Up to date: X 1.2 matches Y 1.2 / installed newer than live metadata / modified date matches / No comparable update data was found. / Out of date: 1.0 is below 1.3。
- **E6 (P0)** 禁用/启用：把该模组所有 DLL 在 `.dll` 与 `.dll.disabled` 间改名；目标已存在则报错 “Cannot rename 'X' because 'Y' already exists.”
- **E7 (P0)** 禁用态视觉：整行文字 danger 红、左缘 2px danger 竖条，动作按钮显示 Enable。
- **E8 (P0)** 批量：Disable all / Enable all（递归匹配两种扩展名）；Delete all（删空 Plugins，破坏性确认，tooltip 写明不动受保护资源与设置）。
- **E9 (P0)** 单项删除：按来源路径删除（目录或文件）。
- **E10 (P0)** 三列布局：每列可从 8 种视图任选 —— Enabled Mods / Disabled Mods / Sharecode Requested Mods / Missing Dependencies / Update Available / Incompatibility / Known Bugs / Needs Attention；默认 启用 / 禁用 / 分享码请求。
- **E11 (P1)** 第三列（分享码）可手动隐藏/显示（开关持久化）；隐藏时网格第三列宽度归零。
- **E12 (P1)** 本地搜索框：匹配名称、GUID、依赖标签。
- **E13 (P1)** Install mod 按钮（选本地压缩包）+ Refresh（重扫并重新比对元数据）。
- **E14 (P1)** 行内动作按上下文出现：Toggle（Enable/Disable）、DependencyActionLabel（Download / Open Download）、Ignore（依赖）、Out of date（打开 Nexus Files 页或来源页）、ShareCodeActionLabel（Open Download / Search Nexus）、Ignore（分享码）、Delete；来源为 GitHub 时“Out of date”打开 release 页。
- **E15 (P1)** 未匹配元数据的模组：名称用文件名去扩展，版本取第一个未禁用 DLL，“Not listed in the community catalogue, so update status is not checked.”。
- **E16 (P2)** 版本证据缺失、GUID 缺失的模组仍要能显示与操作，不能因缺数据整页失败。
- **E17 (P1)** 匹配算法：DLL 名与元数据 `DllNames` 忽略大小写求交；优先“归一化名 == DLL 茎名”，其次 `DllNames` 只有一项的，最后按 `DllNames` 数量升序（避免同名不同模组串台，例如自带 CUCoreLib.dll 的模组）。

### F. 压缩包安装（安装计划）

- **F1 (P0)** 支持 zip/7z/rar；解到临时 staging，生成计划后删除 staging。
- **F2 (P0)** 分类：含 `BepInEx`/`Plugins` 目录 → BepInExLayout（保留布局，装到游戏根或 Plugins 父级）；只含 DLL → PluginDll（装到 Plugins 根）；含 `experimentCrus.png` → CustomSprite；其余 → Unsupported（拒绝并说明）。
- **F3 (P0)** 覆盖检查：列出将被覆盖的目标文件（含目标已有 `.dll.disabled` 的情形），安装前警告。
- **F4 (P0)** 安装时复制除 `.txt` 之外的全部文件。
- **F5 (P0)** CustomSprite：槽位目录必须是 `^st\\d+$`；找到 `experimentCrus.png` 后向上取最近的含 `Head/` 或 `Body/` 的父目录为源根，**不压平**目录结构；目标是 `CustomSprites/stN`，先删后拷。
- **F6 (P1)** 依赖提示：把压缩包 DLL 名与元数据 `DllNames` 求交得到模组名，查依赖目录；“Known required library/libraries: … 安装前请自行安装，本版不自动下载依赖”。无目录条目则提示“依赖未经验证，需人工确认”。
- **F7 (P1)** 多人模组识别：文件名含 Krokosha/CasualtiesMP/Multiplayer 时警告“需按专用说明安装，将解到游戏根而非 Plugins”。
- **F8 (P1)** 皮肤槽位对话框：显示每个槽位已占用的精灵数量；空槽安装不再提示覆盖。
- **F9 (P0)** 路径安全：拒绝绝对路径、`..`、盘符、符号链接穿越；任何写入目标必须 `is_inside(Plugins|游戏根|CustomSprites)`。
- **F10 (P1)** 安装完成后通知本地模组列表刷新（缓存失效）。
- **F11 (P1)** 安装失败要区分“解包失败 / 计划不支持 / 写盘失败”，并保持 Plugins 不被半成品污染（先 staging 校验，再落盘）。

### G. 依赖 / 兼容 / 已知 bug 数据

- **G1 (P1)** 内嵌依赖目录 JSON：`{dependencies:[{mod, requires:[{name, minimumVersion, note}]}]}`；键名归一化（只留字母数字、小写）匹配。
- **G2 (P1)** 内嵌不兼容目录 JSON：`{incompatibilities:[{modA, modB, note}]}`，双向匹配。
- **G3 (P1)** 缺失依赖：只对启用中的模组计算；列在行内 “Missing: … Please install before launching.”；提供 Download / Open Download / Ignore（Ignore 持久化到设置）。
- **G4 (P1)** 冲突：“Incompatible with mod X”；已知 bug：“Known bug: …”。
- **G5 (P1)** 数据文件覆盖顺序：可执行文件旁 `Data/Catalogs/` > 内嵌资源 > 旧路径；按“文件变了才重载”缓存。
- **G6 (P2)** 社区兼容性规则解析器（`type|modA|modB|severity|message`，type ∈ conflict/bug/manual）：上游设置里仍保留缓存字段，但订阅源在 0.0.9 已移除。**建议不复刻**，除非要兼容旧配置。

### H. Modlist 分享码

- **H1 (P1)** 生成：取未禁用且有 GUID 的模组，按 GUID 去重、按名排序，输出 `CUH1:` + 逗号分隔的 `GUID~metadataId@version`（URI 转义）；复制进剪贴板并提示。
- **H2 (P1)** 导入：校验前缀（“That is not a Casualties Hub modlist code. It must begin with CUH1:.”）；兼容旧格式 `GUID@version` 与四段式；结果落 `<appdata>/ImportedModlist.json`。
- **H3 (P1)** 只识别缺失项，不静默下载：缺失项在本地模组页以紫色呈现，动作 “Open Download”（有精确 Files 页标识）或 “Search Nexus”（无标识），可 Ignore。
- **H4 (P1)** 去重匹配规则：导入项与已安装项在 GUID / metadataId / 名称任一相等时视为同一项（避免误报缺失）。
- **H5 (P2)** 分享码按钮与粘贴框放在 Local Mods 顶栏；tooltip 说明“只是清单，不下载任何东西”。

### I. 受保护资源

- **I1 (P1)** Protect files… / Protect folder…：只能选 Plugins 内的项；目录整拷、文件拷贝到 `<appdata>/<ProtectedFilesPath>/<相对路径>`；同相对路径去重；清单存 `ProtectedFiles.json`。
- **I2 (P1)** Restore all：按相对路径深度升序，先删目标当前版本再从备份拷回；返回还原条数并提示。
- **I3 (P1)** Remove：只删备份与清单项，绝不动游戏内文件；校验备份路径在保护根内。
- **I4 (P1)** 列表项显示 DisplayLabel（文件/文件夹 + 相对路径）与 SavedPath；Open folder 打开备份根。
- **I5 (P2)** 空态说明：解释“保护的是你自己改过的皮肤等，重装或更新不会丢”。
- **I6 (P1)** 越界错误提示：“Choose files or folders inside the resolved Plugins folder.” / “The protected item has an invalid saved path.”。

### J. 备份

- **J1 (P1)** Back up now：把 Plugins 全部内容复制到 `<backupRoot>/yyyy-MM-dd_HH-mm-ss`（绝对路径配置则用之，否则 appdata/Backups），不动原文件，提示“有备份总是安全的”。
- **J2 (P1)** 列表：标题（时间戳）、摘要（文件/目录数）、路径；动作 Open / Restore / Delete。
- **J3 (P1)** Restore：覆盖式拷回（同名覆盖，备份中不存在的新文件保留）；需要确认。
- **J4 (P2)** Delete：只删备份目录，需要确认。
- **J5 (P2)** 空态与刷新。

### K. Skins & Backups（皮肤预览）

- **K1 (P1)** 页面合并三个标签：Skin Preview / Protected Assets / Backups。
- **K2 (P1)** 槽位发现：`Plugins/CustomSprites/st\d+`，只保留精灵数 > 0 的槽位；Head/Body 各自统计 PNG 数（大小写不敏感）；按数字排序（st10 不能排在 st1 与 st2 之间）。
- **K3 (P1)** 列表项：`ST0`、`3 head · 12 body`；缺必需精灵时红色并 hover 列出缺失文件名。
- **K4 (P1)** 合成：按游戏内 17 个 SpriteRenderer 节点的局部变换（Head/Eyes/UpTorso/DownTorso/Tail/双臂/双手/双腿/双脚，含 Eyes 依附 Head、Tail 依附 DownTorso 的父链合成），8 px = 1 Unity unit；先量出旋转后的真实包围盒做画布，再按 SortingOrder 叠放；像素插值必须关（否则糊成一片）；尺寸取像素尺寸而非 DPI 尺寸。
- **K5 (P1)** 头部下拉：普通头 + 张嘴/半张嘴 + 三种毁容头（治愈/未治愈）。
- **K6 (P1)** 表情下拉：None / Open / Closed / HalfClosed / Happy / Sad / Scared / Panic / Gone / GoneHealed。
- **K7 (P1)** 朝向：Turn around 整机镜像（X 取反、旋转角取反，缩放与位图一起翻），不使用 `*Back` 精灵。
- **K8 (P1)** 交互：整身自适配、拖拽平移、缩放滑块 + 滚轮、Centre 复位（重置缩放与滚动）。
- **K9 (P1)** 精灵查找：Head 找不到时到 Body 找（游戏加载器按文件名合并两个目录）。
- **K10 (P2)** 缺必需精灵清单：12 个基础件（headBack/eyeOpen/upTorso/downTorso/upArm/downArm/handF/handB/thigh/crus/foot/tail）；缺的可选表情不算安装错误。
- **K11 (P2)** 外链社区皮肤站；槽位缺失时给空态。
- **K12 (P2)** 单槽损坏 PNG 不能让整页崩溃（跳过该件、记日志）。

### L. Hub Home（公告与发布信息）

- **L1 (P1)** 顶部公告卡：发布者公告正文 + 发布时间；Check now（30s 冷却，按钮显示 `Check now (23s)` 倒计时）。
- **L2 (P1)** Previous announcements：本机保存、可回看最近 3 条（即使订阅源已不再列出）。
- **L3 (P1)** What changed in this build：从内嵌发布说明读 “What changed” 列表（`-` → `•`）；无文件时 “What changed notes are not available for this build.”。
- **L4 (P1)** Release information（可展开）：“Release information” 段，回落 “Testing note”；都无则 “No additional release information is available.”。
- **L5 (P1)** Links：Release history / Nexus page / Discord（配置里没有则禁用并 tooltip）/ Credits。
- **L6 (P1)** 配置频道：预发布构建读 `v1/prerelease/hub.json`，正式构建读 `v1/hub.json`；30 分钟刷新间隔，聚焦时推迟；聚焦即读缓存不发请求。
- **L7 (P1)** 只接受 https 链接，非 https 直接丢弃并隐藏对应按钮；公告必须有 Id + Message。
- **L8 (P2)** 在线/离线指示：绿/红状态点或文案“Announcements up to date.” / “Could not reach the Hub configuration.”。
- **L9 (P2)** 版本行：“Installed version: x.y.z”。
- **L10 (P2)** Credits：方形贡献者卡片（姓名、角色、描述、照片位；缺照片用同尺寸 “Pending Picture” 占位）。

### M. Nexus 凭据与直连下载

- **M1 (P1)** 保存/清除个人 API key：输入框密码态，Save / Clear；空值即清除。
- **M2 (P1)** 落盘加密：信封 `CHK1` + 12B nonce + 16B tag + AES-256-GCM 密文；目录 0700、文件 0600；Windows 用 DPAPI(CurrentUser) 包裹密钥，其它平台裸存 0600；解密失败提示重新输入（不崩）。
- **M3 (P1)** 兼容旧格式（裸 DPAPI blob），读取时重写为新格式。
- **M4 (P1)** 直连下载：`GET /v1/games/scavprototype/mods/{id}/files.json` → 选 `category_id` 1（MAIN）或 2（UPDATE）里 `uploaded_timestamp` 最新者 → `…/files/{fileId}/download_link.json` → 取 `URI` 下载到下载目录；文件名做非法字符净化；超时 5 分钟。
- **M5 (P1)** 错误文案：无 key / 403 / 非 Premium 分别给可操作提示，并且**永不**伪装成能下载。
- **M6 (P2)** 进度反馈：下载中状态、完成后提示“已放入下载目录，收件箱会询问是否安装”。

### N. Steam 启动

- **N1 (P0)** 必须经 `steam://rungameid/{appId}`（Proton 下直接跑 exe 会跳过兼容层，BepInEx 不加载）；先由游戏根反查 app id（比对各库已装游戏，或从路径向上找 `steamapps` 读 manifest 匹配 installdir）。
- **N2 (P0)** 回退链：系统 handler 打开 URL 失败 → `steam -applaunch {appId}`（Windows 用注册表 Steam 根下 steam.exe）→ 报错 “Could not reach Steam…”（含 app id）。
- **N3 (P1)** 找不到 manifest 时文案：“…cannot be launched through Steam. Start it from your Steam library instead.”。
- **N4 (P1)** Steam 库发现：Windows 注册表 3 处 + Program Files 兜底；Linux 6 处（含 Flatpak）并解符号链接去重；读 `steamapps/libraryfolders.vdf` 与 `config/libraryfolders.vdf`（现代 + 旧式两种布局）。
- **N5 (P1)** VDF 解析器：真 tokenizer（引号字符串、C 风格转义、`//` 注释、对象嵌套、无值键、深度上限 32）。
- **N6 (P1)** 状态栏反馈：“Asked Steam to launch Casualties Unknown.” / “Could not launch: …”。

### O. 外观系统（像素风）

视觉基准是游戏内 UI（截图存于会话记录）：纯黑底、面板悬浮、1px 硬边、方角、无渐变、无阴影、无过渡动画。应用窗口本身就是黑底，**不显示游戏画面**，也不做半透明／毛玻璃。

- **O1 (P0)** 调色板固定，用户不可改写（色值取自游戏 UI 的像素采样，非臆造）：

| token | 值 | 用途 |
|---|---|---|
| `bg` | `#000000` | 窗口与页面底 |
| `surface` | `#0B0B0B` | 面板底 |
| `surface_raised` | `#141414` | 列表行、输入框、状态栏 |
| `border` | `#FFFFFF` | 面板／对话框 1px 外框 |
| `border_dim` | `#3A3A3A` | 行分隔线、内框、禁用描边 |
| `text` | `#F2F2F2` | 正文 |
| `text_muted` | `#A0A0A0` | 作者、统计、次要说明 |
| `text_dim` | `#6D6D6D` | 占位符、禁用文字 |
| `accent` | `#39DE82` | 已安装／可用／数值／主按钮 |
| `accent_press` | `#168301` | 按下态、强调底 |
| `warn` | `#FFF622` | 有更新、警告、倒计时 |
| `danger` | `#FF0000` | 错误、破坏性操作、禁用标记 |
| `link` | `#00FFFF` | 外链、焦点框 |
| `request` | `#FF66FF` | 分享码请求（游戏调色板外的唯一补充色） |

- **O2 (P0)** 语义映射：绿=可用／已装／正向；黄=更新／警告／计时；红=禁用／错误／破坏性；青=链接／焦点；品红=分享码请求；灰阶只表达层级，不表达状态。
- **O3 (P0)** 字体：内嵌「Fusion Pixel 12px」（OFL，含中英日字形），缺字形回落「Ark Pixel 12px」，再回落系统等宽；字号只有 12/24/36 三档（= 12px × 缩放）；1x 下正文行高 16px、列表行高 24px。禁用字体平滑与亚像素定位，文本基线必须落在整数像素。
- **O4 (P0)** 缩放：1x / 2x / 3x，默认 2x。所有尺寸按整数倍放大，1px 描边在 2x/3x 下为 2px/3px（保持 1 逻辑像素）；图标 16×16 逻辑像素，只做整数倍。
- **O5 (P0)** 面板规格：外框 1px `border`，向内让 2px 再画 1px 内框；标题嵌在上边框中央，左右各留 8px 断口（游戏里的「需求」「信息」「SAVE SLOTS」即此做法）；分节标题用 `warn` 或 `text`。
- **O6 (P0)** 控件规格：按钮单框 1px，1x 高 28px，hover 反色（底色 `text`、文字 `bg`），按下整体偏移 1px；输入框 1px `border_dim` + `surface_raised` 底，聚焦换 `link` 框；菜单与对话框同面板规格，无阴影。
- **O7 (P0)** 列表与表格：1x 行高 24px，行间 1px `border_dim`；hover 行底色 `surface_raised`；选中行文字 `accent`；列表头 `text_muted` 全大写拉丁或中文短词；滚动条 4px 滑块 `text_dim`，无箭头、无圆角。
- **O8 (P0)** 状态呈现：徽标与状态文本只用颜色 + 卡片左缘 2px 同色竖条，不铺大色块；禁用行整体 `danger`。
- **O9 (P1)** 图标：16×16 单色 1-bit 像素图集，运行时按 token 着色；不使用矢量图标与字体图标（上游 0.0.9-pre.2 引入的 material icons 不复刻）。
- **O10 (P1)** 动效：不做补间、不做淡入淡出；只允许三种即时反馈——按下 1px 位移、hover 反色、载入中三点循环（`·`→`··`→`···`，200ms/帧）。Animated RGB、色相旋转、20FPS 全量重绘全部取消。
- **O11 (P1)** tooltip 规范（沿用上游语义）：纯文本标签不弹；只有按钮与被截断文本可 hover；空白 tooltip 抑制；批量／破坏性按钮必须写清作用范围。样式为黑底 1px 白框、最多两行。
- **O12 (P2)** 高对比模式：`bg #FFFFFF` / `text #000000` / `accent #007A33` / `warn #7A6A00` / `danger #B30000` / `link #005A7A`，其余 token 同步反转，供弱视与强光环境。
- **O13 (P2)** 不复刻：四色派生（`Mix`／`Shift`）、WCAG 运行时自动改色、色轮取色器、四个自定义预设槽、Animated RGB、10–20 字号滑块、圆角卡片与柔和阴影、白色 tooltip 底。

### P. 设置与迁移

设置页分组（每组一节，字段逐项复刻）：

- **P1 (P0) Folders**：Game folder（文本框 + Browse… + Detect）、Downloads folder（文本框 + Browse…）、Keep imported archives 开关（tooltip 说明“关=装完就移走”）。
- **P2 (P1) Appearance**：界面缩放三档（1x / 2x / 3x，默认 2x，改后即时生效）、高对比模式开关（见 O12）、一行说明“界面配色固定，跟随游戏风格”；**不**提供字号滑块、取色器与自定义预设（见 O13）。
- **P3 (P1) Nexus Mods**：key 输入 + Save / Clear + 说明“Premium 才能直连下载，可选”。
- **P4 (P1) Extras**：Enable easter eggs（“下次启动生效”）。
- **P5 (P1) Diagnostics**：说明文字、Copy report、Open logs folder。
- **P6 (P0)** 设置模型字段（全部要落盘）：gamePath、downloadPath、backupPath("Backups")、disableAutoDeleteImportedParentFiles、uiScale(2)、highContrastMode(false)、protectedFilesPath("ProtectedFiles")、easterEggsEnabled、easterEggsPreferenceInitialized、localModsShareColumnCollapsed/Visible、dashboardListView、dashboardShowAdultMods、ignoredDependencyNames[]。旧字段（textSize、四个颜色、activeUiPreset、animatedRgbEnabled、customUiPresets、themeColoursInitialized、rebrandThemeInitialized、缓存兼容性字段）读取时忽略、保存时丢弃。
- **P7 (P0)** 容错：JSON 解析失败不覆盖原文件，本次用默认值；数值越界夹紧（界面缩放 1–3、缩放档位非法则回落 2x）。
- **P8 (P1)** 写入用缩进 JSON，便于用户手改。

### Q. 日志 / 诊断 / 崩溃报告

- **Q1 (P0)** 每会话一个 `Logs/Log yyyy-MM-dd_HH-mm-ss.log`；三级 INFO / ACTIVITY（`[Area] msg`）/ ERROR（含完整异常）；内存里保留最新 500 条供页面显示（新→旧）。
- **Q2 (P0)** 保留策略：会话日志、诊断日志、崩溃报告各留最近 5 个，超出删除最旧。
- **Q3 (P0)** Debug 页：实时活动列表（等宽字体）、Auto-scroll、Refresh、Clear view、Copy log、Open current log、Open BepInEx crash log（`LogOutput.log`，找不到再试 `LogOut.log`）、Create crash report、Open logs folder。
- **Q4 (P0)** 崩溃报告内容：Hub 版本、OS、运行时版本、配置与活动模组快照、本会话日志、未处理异常全文。
- **Q5 (P1)** 诊断日志内容：最近 10 分钟活动 + 环境 + 游戏/插件快照。
- **Q6 (P1)** CLI `--diagnostics`（无窗口）：打印版本/OS/用户名（root 标注）、数据目录（非绝对路径要报错）、日志目录、应用目录；检查三个内嵌资源存在与否；Steam 段列出库、app id、真实大小写、DLL 数量与非小写扩展名计数；输出 RESULT 行并以退出码表示成败。
- **Q7 (P1)** CLI `--selftest`（无显示器）：构建全部页面与对话框各一次，报告 OK/FAILED；有皮肤时额外合成一次正面与镜像预览并校验件数（可落 PNG）。等价于 CI 冒烟。
- **Q8 (P2)** 开发者控制台页（隐藏入口）：与 Debug 页共用数据源。

### R. Multiplayer（预览，P2）

- **R1 (P2)** 明确“布局预览”声明：服务器是虚构的、不联网、Join 无效。
- **R2 (P2)** 控件：Hide locked、Refresh、按名称过滤、区域下拉（nae/naw/euw/eue/sa/as/oce）、Ping 阈值（Any/50/100/150）、按 Ping 升序。
- **R3 (P2)** 行：名称、Mood（Casual/Hardcore/Roleplay）、Layer 1-3、Locked、人口 P/C、Ping、模组数、Join。
- **R4 (P2)** 空态 “No sample servers match those filters.”，Refresh 提示 “Sample data reloaded. There is no multiplayer service to query yet.”。

### S. 打包与发布

- **S1 (P0)** 单文件自包含可执行（Windows exe + Linux）；内嵌：依赖目录、不兼容目录、发布说明（`Version {version}.txt`）、离线公告副本。
- **S2 (P0)** `Data/` 目录（可执行文件旁）优先于内嵌资源，允许用户手改目录数据；改动按文件时间戳重载。
- **S3 (P0)** 版本号来自二进制自述（编译期注入），并与发布说明文件名对应；启动时读它决定公告频道（预发布 / 正式）。
- **S4 (P1)** SemVer 解析与比较：`^v?\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$`；数值段按数值、数字段低于字母段、字母段忽略大小写比较；正式版高于同号预发布。
- **S5 (P1)** Linux 包：tar.gz + `README-linux.txt`（chmod 说明、可选 desktop entry）；desktop entry 在含空格路径下也能启动。
- **S6 (P1)** Windows：单 exe；DPAPI 可用；注册表探 Steam。
- **S7 (P2)** 发布流水线检查（若自建）：仅 main、版本可读、说明存在、tag 不重复、版本类型与发布类型一致、产物自报版本一致。

### T. 明确不复刻

- 自更新（0.0.8-pre.6 起移除，改用 Nexus 分发）——若需要，另立 ADR。
- 应用内卸载器（0.0.8-pre.5 加、0.0.9-pre.1 移除）。
- 外部 Developer Console（监视 LocalAppData 执行外部指令）与 `CH Uninstaller.cmd`。
- Supabase 公告、匿名 UUID 安装 ID、活跃安装统计。
- 旧 WPF Hub / Setup Wizard / HubContent.json 订阅源。

### U. 来源表与整合包（P2）

社区目录是从 Nexus 生成的，GitHub 或自托管的模组多半不在里面；来源表就是给这类模组补一条更新与下载的出路，不替代 Nexus。

- **U1 (P2)** 优先级：用户来源表 > 目录可选字段（`github` / `url`）> 元数据里的 Nexus id > 无来源（只显示本地信息，不给更新判定）。
- **U2 (P2)** 三种来源：`nexus`（内置，files.json 选文件）、`github`（内置，`releases/latest` 取 tag 与资产）、`custom`（用户写正则）。
- **U3 (P2)** **只有 GitHub 内置版本解析**：`tag_name` 去掉前缀 `v` 后按 S4 规则与本地 assembly／BepInEx 版本比较；资产由用户给的正则挑，没给就只提供 release 页链接。用户点 `Update` 时把匹配到的资产下到下载收件箱，再由收件箱的确认流程决定是否安装；不点则只给链接，绝不后台下载。
- **U4 (P2)** 自定义来源：`page` URL + `version` 正则（捕获组 1 = 版本）+ 可选 `download` 正则（捕获组 1 = 下载地址，相对地址按 page 解析）。正则用 `regex`，线性时间，允许用户写坏但不会卡死。
- **U5 (P2)** 抓取约束：只 GET、只 https、只带本应用 UA，不带 cookie 与任何凭据，不跟随到 `file:`/`data:`，响应上限 2 MB，超时 15 s，重定向最多 3 次。
- **U6 (P2)** 自定义来源一律拒绝 `nexusmods.com` 及其子域，提示改用 `nexus` 类型——既不抓 Nexus，也不伪装成别的东西。
- **U7 (P2)** 失败态可读：`source unreachable` / `version pattern did not match` / `no asset matched`；保留「Open source page」退路，同一会话不重复重试。
- **U8 (P2)** 来源表存 `appdata/sources.json`（`schemaVersion` + `entries[]`，每项含 `match`（dllNames / guid / name 任一命中）与 `source`）；也可放可执行文件旁 `Data/sources.json` 全局共享。
- **U9 (P2)** 表校验：正则编译失败、URL 非 https、`kind` 未知、`match` 全空的条目整条丢弃并报行号，坏条目不拖垮整表。
- **U10 (P2)** 缓存：来源解析结果 6h + ETag；GitHub 未认证限 60 次/小时，只在「已安装模组且用户打开详情或点检查」时请求。
- **U11 (P2)** 整合包：`CUPK1:` 分享码 = 来源表子集 + 模组名 / GUID / DLL 名 + 期望版本；导入只列缺失项与来源，逐个走「下载 → 收件箱 → 确认安装」既有流程，不静默安装。排在 M8：先把来源解析、缓存与失败态跑通，再做整合包。
- **U12 (P2)** 导出与导入都要确认，且只允许 https 来源；导入前把「会从哪些域下载」列出来。

---

## 3. 本地数据布局（应用数据根）

```
<appdata>/                       # Windows %LOCALAPPDATA%\CasualtiesHub；Linux $XDG_DATA_HOME/CasualtiesHub（退回 ~/.local/share）
  Settings.json
  UniversalMetadataCache.json    # 社区目录 6h
  UniversalMetadataHttpState.json# ETag / Last-Modified
  NexusPopularMods.json          # 可选（GraphQL 缓存 8h）
  HubConfigCache.{stable|prerelease}.json
  HubConfigHttpState.{stable|prerelease}.json
  ImportedModlist.json
  ProtectedFiles.json
  ProtectedFiles/...             # 受保护资源副本
  Backups/yyyy-MM-dd_HH-mm-ss/...
  ImportedDownloads/yyyy-MM-dd_HH-mm-ss_<name>
  IconCache/<sha256 前 32>.img
  Logs/Log yyyy-MM-dd_HH-mm-ss.log
  Crash Reports/…
  NexusApiKey.dat / NexusApiKey.key
```

插件目录只由用户指定，Hub 不复制游戏文件到仓库；所有路径判断都基于“真实大小写解析后的绝对路径 + is_inside”。

---

## 4. Rust + nsh 落点

### 4.1 已验证的 nsh 能力（Rust 侧）

- `nsh_core` Rust app：components / signals / styles，一套模型多平台（Windows、Linux、macOS、iOS、Android、Web）。
- 文件与文件夹：`pick_paths(OpenDialog::files()/folders()/files_and_folders())`、`pick_save_path(SaveDialog)`、`open_path`、`reveal_path`、`move_to_trash`，均为 await 的任务；取消返回 `Ok(None)`。
- 对话框：`Dialog`（自定义浮层，覆盖在原生层之上）与 `AlertDialog.show`（平台原生 alert/action sheet）。
- 剪贴板：`Clipboard.writeText / readText`。
- 存储：`Storage.getItem / setItem`（每 app 私有，无需 capability）。
- 通知：`Notifications.show / on`。
- 图片：`Image` 元素 + `objectPosition`；远程图片受 `capabilities.network.hosts` 限制，本地资源始终允许。
- 主题：`theme` → design tokens（`token()`）；**运行时改色的路径需实测**（见 4.3）。
- 打包：`nsh run/dev/check/build/package`，`nsh.json` 里声明 capabilities。

### 4.2 需要 Rust 自己实现的部分（nsh 不提供）

| 需求 | 建议 |
|---|---|
| HTTP + 条件请求 | `reqwest`（rustls）+ `ETag`/`If-Modified-Since`；超时 15–20s；UA `CasualtiesHub/{version}` |
| JSON | `serde`/`serde_json`；社区的 `[JsonExtensionData]` 容错字段用 `#[serde(flatten)] HashMap<String, Value>` + 大小写不敏感键查找 |
| 版本提取（自定义来源） | `regex`（线性时间，无回溯爆炸）；用户正则的捕获组 1 即版本／下载地址 |
| GitHub Releases | 只调 `api.github.com/repos/{o}/{r}/releases/latest`，取 `tag_name` 与资产名；未认证 60 次/小时，必须缓存 |
| zip / 7z / rar | `zip`、`sevenz-rust`、`rars`（纯 Rust RAR，库依赖，不调外部二进制）；统一抽象成 `ArchiveReader`，出口做 zip-slip 校验；RAR 解不出时再尝试 PATH 上的 `unrar`（可选兜底，需用户已安装） |
| 目录变化监视 | `notify`（inotify / ReadDirectoryChangesW / FSEvents）；Windows 用独占打开判定完成，Linux 用稳定大小×3 |
| PNG 解码 + 合成 | `image` 解码 + 自己的变换（平移/旋转/缩放/排序/ZIndex），输出位图给 nsh `Image` |
| .NET 程序集元数据 | `dotnetdll` 0.4（读写 CLI metadata）：`Assembly` 表取版本、Custom Attribute 解 `BepInPlugin(string,string,string)` blob（prolog `0x0001` + 3 个 SerString）；只读不加载。解不出时不自己写 CIL 解析器，降级只读 Assembly 版本 |
| 凭据加密 | `aes-gcm` + 随机 nonce；Windows 用 `windows` crate 调 `CryptProtectData`/`CryptUnprotectData`（DPAPI CurrentUser）；Linux 落 0600（可选 `keyring` 走 Secret Service） |
| Steam 库与 VDF | 自写 KeyValues tokenizer（照上游规则）+ `steamlocate`（或自写路径扫描）；注册表用 `windows`/`winreg` |
| 进程启动 / URL | `open` crate 或 `std::process::Command`；scheme 白名单 http/https/steam |
| 日志与崩溃 | `tracing` + 自定义文件 writer（会话文件、保留 5 份）；`std::panic::set_hook` 生成崩溃报告 |
| 版本比较 | 手写 SemVer（规则见 S4），不要直接用 `semver` crate 的预发布序（差异点：数字段 vs 字母段的比较方向） |
| 平台差异 | 用一个 `platform` 模块收口：appdata 根、Downloads 默认值、DPAPI、注册表、inotify 提示 |

### 4.3 需要先做 nsh 验证（spike）

1. **大列表与网格**：50+ 卡片、封面异步加载、列表内展开详情、保持滚动位置；是否虚拟化、是否支持 ZIndex/浮层（详情浮层压在网格上）。
2. **像素样式与缩放**：固定调色板能否只靠 design tokens 落地（无需运行时改色）；1x/2x/3x 整数缩放下 1px 双框、像素字体字距与描边是否精确对齐（Linux 软件渲染下发虚与否）。
3. **自绘位图合成**：把合成结果交给 `Image` 显示像素级精确（无插值）、可缩放平移（Viewbox 等价物）。
4. **文件监视 + 后台任务 → UI 线程**：`notify` 线程回调如何安全更新 signal/状态栏。
5. **模态与确认**：`Dialog`（自定义）与 `AlertDialog`（原生）在 Windows/Linux 的表现，以及“确认后才执行”的 await 链。
6. **窗口装饰**：Linux 客户端装饰、标题栏自定义、拖拽区域。
7. **单文件打包 + 内嵌资源**：资源作为 Rust `include_bytes!/include_str!` 还是外部 `Data/` 覆盖；二进制体积。
8. **CLI 参数**（`--diagnostics`/`--selftest`）与无显示器自检：nsh 是否有 headless 测试模式（文档有 `nsh test`，需确认能否无窗口构建页面）。

### 4.4 建议的 crate / 模块骨架

```
cuhub/
  crates/
    app/            # nsh UI：components、pages、状态栏、导航、palette.rs（调色板 token 与缩放）
    core/           # 领域模型 + 设置 + 服务（无 UI 依赖，可单测）
      paths.rs      # appdata / resolve_chain / is_inside / downloads
      settings.rs   # 模型 + 迁移 + 容错
      nexus.rs      # 直连下载（files.json / download_link）
      metadata.rs   # 社区目录 + 缓存 + 条件请求
      hubconfig.rs  # 公告频道 + 缓存
      mods.rs       # 扫描 / 版本证据 / 更新判定 / 开关
      archive.rs    # 分类 / 计划 / 安装 / 覆盖检查
      catalogs.rs   # 依赖 / 不兼容目录
      modlist.rs    # CUH1 编解码
      sources/      # 来源解析：nexus / github / custom + 来源表 + CUPK1
      protect.rs    # 受保护资源
      backup.rs
      skins.rs      # 槽位发现 + 骨架合成
      steam.rs      # 库扫描 / appid / 启动
      logs.rs       # 会话日志 / 崩溃报告
      cli.rs        # --diagnostics / --selftest
  docs/FEATURES.md
  nsh.json
```

核心服务必须**不依赖 UI**：上游最值得照搬的架构决定就是把所有危险操作放在 service 里，UI 只做展示与确认；这样 `--selftest`/`--diagnostics` 与单元测试才有意义。

---

## 5. 里程碑建议

| 里程碑 | 内容 | 验收 |
|---|---|---|
| M0 骨架 | nsh 工程、导航、主题系统、设置读写、状态栏 | 能在 Linux/Windows 起窗口、切 6 页、设置持久化 |
| M1 游戏路径 + 本地模组 | B、E1–E9、E17 | 指向真实游戏目录能列出模组、开关、删除、更新判定正确 |
| M2 安装与收件箱 | D、F | 下载 zip → 自动询问 → 装进 Plugins；四类分类正确；覆盖有警告 |
| M3 Dashboard + 元数据 | C（除直连下载）、G | 目录 6h 缓存、搜索/过滤/排序/18+/双视图/详情/右键 |
| M4 凭据 + 直连下载 + Steam | M、N | 有 key 走官方 API；无 key 打开 Files 页；Launch Game 生效 |
| M5 分享码 + 保护 + 备份 + 皮肤 | H、I、J、K | 生成/导入分享码；保护→破坏→还原；皮肤预览与游戏内一致 |
| M6 Hub Home + 外观 + 日志 | L、O、Q | 公告频道正确、像素调色板与整数缩放落地、诊断/自检命令可用 |
| M7 打磨 | P2 项、多人占位、打包发布 | 单文件产物 + README-linux + 资源内嵌 |
| M8 来源表（P2） | U | GitHub tag 能判更新、自定义正则能读出版本与下载、坏条目被丢弃且有行号、CUPK1 导入只走确认流程 |

---

## 6. 风险与待定

- **K 皮肤合成**是最容易“看起来像但不对”的部分：节点变换、像素单位、镜像方向必须逐项对齐上游常量（本文已列 8 px/unit 与 17 节点），否则用户一眼看出错位。
- **.NET 程序集读取**：`dotnetdll` 覆盖 `Assembly` 版本与自定义属性解码，但它仍偏年轻；若 `BepInPlugin` 的 blob 解不出，只读 Assembly 版本并显示“No version evidence found”，不自己写解析器。
- **像素字体覆盖**：Fusion Pixel／Ark Pixel 均为 OFL 可内嵌，但生僻汉字与特殊符号可能缺字形，需要回退链与缺字日志，否则界面会出现空白方块。
- **nsh 生态成熟度**：桌面端自绘控件、虚拟列表、运行时主题、文件对话框在 Linux 上的可靠性都要靠 spike 定论；建议 M0 阶段就把 4.3 的 8 项验完，再定是否保留某些视觉细节（如像素字体在 2x/3x 下的字距与描边对齐）。
- **不做自更新**意味着分发靠 Nexus/手动替换；若要加，需要签名与校验设计（不要复活被移除的 zip 替换流程）。
- **社区目录依赖第三方仓库**（`jimmyking9999999/Metadata-generator`）：需要容错、离线缓存与明确的“数据可能不全”提示（上游 README 已列此限制）。

# app 层总览（`crates/app`）

## 职责

nsh 0.5.1 桌面壳：标题栏、侧栏树形导航、状态行、八页内容区（模组树下的 Nexus / 远程模组 / 本地模组，加 Multiplayer / Skins & Backups / Hub Home / Settings / About）、右下角任务按钮与右侧任务中心抽屉（见下文「任务中心」，取代原先散落在页面里的 pending / 依赖面板）与开发者控制台浮层。数据全部来自 `cuhub_core::hub::Hub`；后台任务只写 signal，界面只读 signal（线程模型见 `docs/ARCHITECTURE.md`）。nsh 细节与坑见同目录 `nsh-notes.md`。

## 导航树

- 唯一数据源是 `src/shell/nav.rs` 的 `NAV` 表：每条含 `id`、locale `key`、窄模式 `icon`、可选 `group` 与 `NavKind`（`Page` 渲染函数 / `Action` 副作用）。加页只改表、`pages` 与 locale，不动 `shell.rs`。
- 当前 8 条：Hub Home（默认页）、Nexus / 远程 / 本地（`group = "mods"`）、Multiplayer、Skins & Backups、Settings、About。搜索框固定在侧栏顶部、折叠开关之上，不占 `NAV` 条目。组头来自 `GROUPS`，只翻 `nav_closed_group`，不写 `page`。
- `Ui.page` 是 `Signal<&'static str>`，存 id；页头、状态行与开发者浮层统一用 `nav::title(id)`，组内条目自动拼「模组 / 子页」。
- 侧栏两态：宽 176px（组头 28、子项 26、缩进 16 + 1px 竖线 + 8）与窄 44px（只有图标，组间 1px `BORDER` 分隔）；开关在 `nav_collapsed`，切换时先 `animate_layout` 请求一次原生布局动画（近临界 spring，见「动画机制」），宽度在 176 / 44 之间插值；`nav_collapsed` 与 `nav_closed_group` 一样只存内存。
- 页脚是启动键（`NAV` 之外的 `LAUNCH_ENTRY`，走 `Ui::launch_game`）与版本行；版本行五连点开开发者浮层。
- 顶部搜索框当前只写状态行提示，搜索本体与「跳转到具体条目」仍未实现。

## 视觉约束

- 固定 14 色 token（`src/palette.rs`）。不新增颜色，也不做颜色补间：任何中间色都会离开调色板。
- 所有尺寸按 `zoom`（1x/2x/3x，默认 2x）缩放，统一走 `ui::scaled` / `ui::pix` / `ui::panel` / `ui::button`；动画默认只作用于样式层（opacity、位移），不参与布局，也不写死像素。唯一例外是侧栏折叠：宽度是布局尺寸，改走 `animate_layout` 的原生布局动画（近临界 spring）；其余布局仍不做补间。
- 像素风的"fancy"限定为收放、脉冲、闪烁、滑动、逐行出现，不用毛玻璃、渐变、弹跳堆料。

## 动画机制

组件只运行一次，因此动画一律由 nsh 原生声明式 API 驱动，不用 `set_interval` 轮询：

- `motion().entering(MotionPreset::…)`：挂载即播放一次。绑在 `when` / `each` 区域内的元素上，区域销毁时动画随之结束。
- `KeyframeAnimation` + `Keyframes::Pulse`：只在该元素存在期间跑，`frame_rate` 显式压低帧率上限。
- `effect`：signal 变化后触发一次播放，不额外起定时器。
- `animate_layout(LayoutAnimation)`：只作用于随后这一次提交的布局变化。侧栏折叠把 `update` 设为 `LayoutTransition::spring`，宽度由平台逐帧插值；快速连点会从当前帧重新收敛，`reduced motion` 时直接到终态。

| 位置 | 触发 | 效果 | 时长 / 帧率 |
| --- | --- | --- | --- |
| 八个页面根视图 | 切页（`when` 重新挂载） | 淡入 + 自下而上 24pt 滑入 | 450ms，一次性 |
| Hub Home 卡片与公告卡 | 页面挂载；卡片悬停 | 淡入；hover 底色与边框切换 | 一次性 / 即时 |
| 侧栏选中项标签 | 该项被选中（`effect` 观察 `page`） | 标签淡入点亮 | 300ms，一次性 |
| 侧栏折叠 / 展开 | `nav_collapsed` 切换 + `animate_layout` | 宽度 176 ↔ 44 一次布局动画（spring），内容即时切换 | 响应 220ms、阻尼比 0.8，收敛约 300ms |
| 状态行 | `notice` 变为非空 | 提示自左滑入 24pt + 淡入 | 300ms，一次性 |
| 开发者控制台 | 打开浮层 | 遮罩淡入 + 面板滑入 | 300ms / 450ms |
| 删除确认弹窗 | 打开（`when` 重新挂载） | 遮罩淡入 + 面板滑入 | 300ms / 450ms |
| 设置向导 | 打开 / 切步 | 遮罩淡入 + 卡片滑入；步骤内容淡入 | 450ms / 200ms |
| 任务中心抽屉 | 点右下角任务按钮（`task_open` 为真） | 右侧滑入 + 遮罩淡入 | 450ms / 300ms |
| 任务卡 | 卡片进入（`each` 重建） | 自右淡入，逐张错峰 | 45ms/级，最多 6 级 |
| 任务角标 | 有待处理项且抽屉关闭 | DANGER 色方块脉冲 | 900ms 循环，20 Hz 上限 |
| Check 进行中 | `can_check` 为假且已知仓库 | 4 个 3px 方块脉冲追逐 | 640ms 循环，20 Hz 上限 |
| 可更新行的 State 徽标 | `row.can_update()` | WARN 色呼吸 | 1800ms 循环，10 Hz 上限 |
| Nexus 刷新中 | `nexus_refreshing` | 同款方块 spinner + 文案 | 640ms 循环，20 Hz 上限 |
| 行 hover / 侧栏按下 | 指针 | 底色 `surface_raised`、底部分隔线转白；导航按下转 `border_dim` | 即时 |

## 模组行布局

- 证据行与来源行的说明文字放在 `flex_1` + `min_w(0)` 容器里并截断，行内按钮 `flex_shrink_0`。窗口较窄或来源说明很长时先裁掉文字，按钮不会被挤出面板（修掉过 Disable 因按钮在窗口外而点不到的回归）。

## Hub Home 磁贴

- 2 列 `grid`：GAME、MODS、CATALOGUE、BEPINEX 四张半宽卡，公告卡 `col_span_full()` 收尾；入口仍是 `pages::hub_home`，标题与面包屑走 `nav::title`。
- 拖拽用原生 pan：整张卡片（外框）`pan_values` + `bind_translate_x/y` 跟手，`on_pan_end` 按水平/垂直位移换位；拖拽中 `z_index` 抬升、底色 `SURFACE_RAISED`、边框 `ACCENT`，松手 spring 回位。
- 顺序存 `Settings.home_order`（JSON `homeOrder`），经 `Ui::save_home_order` 写回；读取时按固定 id 表忽略未知、去重、补齐缺失，旧设置缺键即默认顺序。
- 公告在启动时由 `Hub::hub_config_cached()` 读一次缓存存入 `Ui.announcement`，渲染路径不取 Hub 锁、只读缓存不联网；无当前公告显示 `hub.not_loaded`，不报错。GAME 卡副文本同时显示游戏目录与 app data 路径。

## 远程模组页

- 数据来自 `Hub::sources()`（core 启动时装载），页面不联网；行模型在 `src/ui/remote.rs`，与已装模组按 matcher（guid / dll / name）关联，同优先级取 guid 最优。
- 状态都在 `Ui` 上：`remote_all` / `remote_rows` / `remote_page` / `remote_total` / `remote_query` / `remote_filter` / `remote_busy`；搜索、过滤与翻页只重建内存切片，不写设置。
- 安装与更新共用 `pending_install`：准备阶段写 `remote_busy`（该行按钮与其它行一起禁用），成功后进任务中心的任务卡（右下角按钮打开；抽屉不会自动弹出），卡片里的 Apply 走既有的 `apply_pending`，Cancel 走 `discard_pending` 删 staging；`refresh_mods` 同时重建本地与远程行，装完按钮立即变成 Update。
- 依赖门禁在 `ui::install::prepare_remote_pending` 与 `prepare_update`：core 返回 `HubError::Dependencies` 时不进 pending，而是写 `dependency_prompt`（目标条目可选 + 问题列表）与状态行提示。渲染路径只读该 signal，不取 Hub 锁。
- 依赖卡列 `Missing` / `Outdated`：有清单条目的缺失项可以 `Install missing`，worker 依次准备并 Apply 每个依赖，清 staging；首次安装随后自动重新准备目标条目，更新则清掉提示并让用户再点一次更新。未知依赖或版本不足只显示手动处理说明，不自动安装。`Cancel` 清掉提示，不触碰已装文件。卡片与其他任务卡同处右侧抽屉，页面内不再有依赖面板。
- 空态分三种：清单缺失（一行说明 + 清单路径小字）、清单无条目、过滤后为空。
- 顶部「刷新列表」按钮调用 `Ui::refresh_sources_now()`：worker 里以 `force=true` 调 `Hub::refresh_sources`，绕过 ETag；成功后同时重建本地行与远程行（本地行的来源判定立即跟上新清单），期间按钮换成 spinner，结果只写状态行 notice。清单缺失的空态也显示该按钮。

## 模组启用/禁用

- 行内 Toggle：禁用行显示 `Enable`、启用行显示 `Disable`。点击后在后台线程调用 `Hub::set_mod_enabled`，成功后重新扫描并把改名的 DLL 数写进状态行；失败只把 `HubError` 文案写进状态行，不弹窗。
- 面板批量：`Disable all` / `Enable all` 调 `Hub::set_all_mods_enabled`，同样后台执行并回写改名数。任一变更进行期间，行内 Toggle、三个批量按钮（含 `Delete all`）与 `Refresh` 一起置灰，避免重复点击。
- 后台任务挂在 App 组件 owner（`Ui::spawn_worker`）上，而不是按钮所在分支：按钮在忙时会整块换成禁用副本，分支 owner 销毁后挂在它下面的任务会被取消，忙碌信号永远不复位。
- 禁用行视觉（FEATURES E7）：左缘 2px danger 竖条由 `status_color` 提供；名称、版本、作者、证据与详情文字转 danger，来源行在带 warning 时仍保留 WARN，避免丢掉来源匹配提示。
- 行 key 里包含 `disabled`，刷新后 `each` 会重建该行，静态文本与按钮才能跟随状态变化。
- 启用与禁用都是可逆操作，不做确认弹窗；删除是破坏性操作，走下一节的确认弹窗。

## 模组删除

- 入口：每行来源行末尾的 `Delete`（启用行与禁用行都有），以及表头的 `Delete all`（危险色，和 `Disable all` / `Enable all` / `Refresh` 同一行、一起换行）。
- 两段式：点击后先起后台线程取范围（`Hub::delete_scope` / `Hub::delete_all_scope`），成功才弹出确认遮罩；取范围失败只把 `HubError` 文案写进状态行，不弹窗。取范围期间相关按钮全部置灰并防连点。
- 确认弹窗列出 core 返回的全部绝对路径（每条一行，路径多时列表滚动，不撑破窗口）、递归文件数，并写明不可回滚。标题区分 `DELETE <名称>` 与 `DELETE ALL MODS`，按钮是 `Cancel` 与红色确认按钮（`Delete` / `Delete all`）。
- 范围完全由 core 决定：单项只删该模组的来源路径；`Delete all` 删除 Plugins 顶层全部条目（含 `notes.txt` 这类非模组文件）。不碰 BepInEx、core 文件、设置与受保护资源，越界路径由 core 拒绝。界面只展示 core 给的路径与文件数，不自行拼路径。
- 确认后仍在后台执行（`Hub::delete_mod` / `Hub::delete_all_mods`）：成功关闭弹窗、重新扫描并刷新行与空状态，状态行写删除的文件数；失败同样关闭弹窗，并把带路径的错误文案完整写进状态行（scope 可能已过期，重试前重新打开确认）。`Cancel` 不做删除。
- 忙态：取范围与执行删除期间，所有 `Delete` / `Delete all` / 行内 Toggle / 两个批量按钮 / `Refresh` 一起置灰；`Cancel` 在删除执行期间同样置灰，避免中途隐藏进度。游戏运行时 core 在删除时才返回 `HubError::GameRunning`，因此这条错误在确认之后出现。
- 弹窗挂在根视图尾部（`pages::delete_overlay`），绝对定位盖住整页内容；排在开发者浮层之前，dev 浮层仍可盖住它。

## 设置向导

- 表面：`setup::wizard` 在根视图 `delete_overlay` 之前，是一层 `when(open)` 挂载的全窗绝对定位子层（`inset 0` + 不透明 `BG`），盖住并阻断 shell；不用 `overlay()`（见 `nsh-notes.md`）。
- 触发：`Ui::new` 里 `Settings::setup_due()`（未完成向导且 gamePath 为空）为真时自动打开，并跳过 A8 的空路径自动检测；设置页顶部的 Run setup again 按钮随时重开，重开时回到第 1 步并重置草稿与内联错误。
- 结构：六步——欢迎、游戏目录、BepInEx（只读）、下载目录、外观、完成；顶部 Step n/6，底部 Back / Next / Skip，完成页为 Finish。步骤内容用 `dynamic` 跟随 `step` 重建，切步 200ms 淡入；遮罩与卡片沿用 FadeIn / FadeInUp。
- 行为：每步 Next 即时保存（游戏目录走 `Hub::set_game_folder`，下载目录只接受已存在的绝对目录，空串表示跟随系统下载目录）；保存失败把错误原文内联在向导里，不前进也不改内存。BepInEx 步只展示状态，不安装、不改文件；外观步复用设置页的缩放与语言按钮。
- 关闭：Esc 与 Skip / Finish 都写 `setupCompleted=true` 后关闭，不二次确认；写入失败则停在本步并内联报错。

## 桥状态与联机控制

- `ui::bridge::watch_bridge` 随其他后台任务启动，每 2 秒取一次 `Hub::bridge_snapshot`：检测到游戏进程才启动桥、退出后停止，会话存在时顺带取 `mp.state`；值不变不写 signal。单次 `invoke_bridge` 最多持 Hub 锁 2 秒，轮询不更密。
- 状态栏右侧显示桥三态与最近一次握手失败；Local Mods 行按 hello 的 guid 快照标「运行中已加载」；Multiplayer 页顶部桥卡片显示 available / running / client / server / host 与 KrokMpVersion，并带「游戏内控制」开关（只授权 `dev.cuhub.bridge` 的 `mp.host` / `mp.join`）。
- `ui::mp_control` 的 `watch_mp_control` 每 2 秒读同一个桥快照：`status.session.pid` 变化重置门控，房主只自动开房一次，客人在隧道 `Connected` 后最多 3 次（0/3/8 秒），`denied` / `invalid_params` 立即停；确认对话框打开或其它任务忙碌时不自动。用户也能随时关掉开关停下自动逻辑。
- 房间卡「游戏内房间」区块：状态来自 `game_phase`（未连接 / 不可用 / 未授权 / 等隧道 / 可开房 / 可加入 / 已开房 / 已加入）；地址房主固定 `10.144.144.1:<port>`，客人取隧道 `virtual_ip:port`；手动开房/加入走两段式确认，失败行可重试，命令期间是 `MpBusy::Game`。
- 桥未运行、端口被占、版本不符都只反映到状态，不影响游戏启动、安装、删除等既有流程。
- Local Mods 卡片底部是独立的「安装桥」区块（`pages/local.rs::bridge_section` 与 `ui/bridge_install.rs`）：显示未安装 / 已安装版本 / 运行中已加载；按钮优先用 exe 同目录或工作区 `dist/bridge/CuhubBridge.zip`，都没有才打开 zip 文件选择器。安装在 worker 里执行，忙态禁用按钮，状态行显示进度与可读错误；成功后刷新 mods 与桥状态，不自动改 profile，桥仍作为普通本地模组行出现。删除模组、批量应用和 BepInEx 安装成功后也会重新读取桥安装状态，避免底部停留在旧的已安装/未安装。

## 任务中心

- 入口：根视图尾部常驻一个右下角浮动按钮（`shell::task_center`，`absolute` + `right/bottom`，随 `zoom` 缩放），点它把 `task_open` 置真；`shell::task_drawer` 是 `sheet().side(Placement::Right)`，宽度 `relative(0.42)`——缩放只有整数档，没有 `max_w`，按比例取宽在最小窗口下仍可读。抽屉标题行含标题与 Close，内容区是 `scroll().flex_1().min_h(px(0.0))`，卡片多了只滚动抽屉，不撑破窗口。Esc、点遮罩、滑动关闭都经 `SheetEvent::OpenChange` 回写 `task_open`。
- 不自动弹：新任务只改变 `pending_count`，让按钮上的角标（DANGER 方块 + 计数）脉冲；`task_open` 只在用户点按钮或关闭抽屉时被改，所以来新计划时界面不会自己跳出来。
- 卡片：`ui::tasks::cards` 每帧从现有 signal 现算（不缓存、不存失败态），五类 `TaskKind`——安装/更新（`pending_install`：标题为模组名，副标题为版本，行显示来源与目标根、替换/新增文件数、通知与冲突、游戏运行中提示，按钮 Apply/Cancel）、依赖（`dependency_prompt`：逐条 Missing/Outdated/手动说明，Install missing/Cancel）、BepInEx（`bepinex_pending`：Settings 的 Install/Repair 生成，按钮 Install 或 Replace / Cancel）、批量（`TaskKind::Batch`：预检写 `Checking dependencies index/total`，执行写当前项 `index/total · name`，按钮 Update selected (N) / Update all (M)，见「批量更新」）、下载（每个 `DownloadTrack` 一张，无按钮）、以及上述卡片的 run-blocked 变体（游戏运行中只标红提示，点 Apply 也只提示一次，不自动重试）。卡片样式是 Win10 风格：`SURFACE` 底、1px `BORDER` 边、左侧 3px 类型色条（安装/BepInEx/批量用 `ACCENT`、依赖用 `WARN`、下载用 `TEXT_MUTED`），右上角小号类型标签。
- 下载进度是真实字节：core 的 `DownloadProgress { written, total }` 经 `DownloadTracker` 写进进程级表（`ui/tasks.rs` 的 `static DOWNLOADS`，`OnceLock<Mutex<Vec<DownloadTrack>>>`），`Ui::watch_downloads` 每 120ms 取一次快照写 `Ui.downloads`，值不变就不写 signal。进度条宽度按 `written / total`，标签是 `percent% · written / total`；`total` 为 `None`（服务端没给 Content-Length）时不显示百分比，条体脉冲、标签只写已下载字节，绝不编造进度。
- 失败：下载/安装/应用失败仍只写状态行（`notice` / `apply_status`）与日志，进度表在 `DownloadTracker` 的 `Drop` 里清掉，所以不会留下失败卡片；重试只能由用户重新发起。

## 批量更新

- 选源：本地页只有 `can_update()` 的行显示复选框（12px 方框 + 勾），勾选键是 `source_key`，存在 `Ui.batch_selection`（`BTreeSet<String>`）；`rebuild_local_rows` 每次重建行都用 `ui::batch::prune` 把不再可更新或已消失的键丢掉，所以禁用、卸载、固定在 Nexus 的行不会留下幽灵选择。表头的 Select updatable (N) / Clear selection 在「全选可更新」与「清空」之间切换，N 是当前可更新行数。
- 入口：任务中心的批量卡片（`TaskKind::Batch`）给两个按钮——Update selected (N) 与 Update all (M)，N 是选中数、M 是可更新数，计数每帧现算；卡片平时不出现，只有存在可更新行或选中项时才在。点按钮才开跑，抽屉不会自己弹。
- 预检：`start_batch` 先把目标拍成快照（item、pin/effective、repo、asset、nexus_id，以及按 repo slug 匹配到的 `SourceEntry`）。真正下载之前逐项预检：有 `SourceEntry` 的走 `Hub::remote_install_issues`（只读清单与已装模组做依赖检查，不下载、不看游戏是否在跑）；没有 entry 的（纯 Nexus 条目在清单里没有对应 GitHub 源）只能走既有 prepare 路径 `ui::install::prepare_update`，这是目前唯一公开的 Nexus 依赖检查入口，代价是预检阶段就会下载该包。
- 聚合：只要任一项返回 Missing / Outdated，整批不开始，预检阶段已下载的 staging 全部丢弃，`dependency_prompt` 放去重后的全部阻塞项（`target: None`），与单条更新共用同一张依赖卡片；用户处理完依赖后重新点按钮才会开跑。
- 顺序执行：干净的一批在 worker 里逐项 prepare → 应用前再查一次（有 entry 的重查 `remote_install_issues`）→ `Hub::apply_package`；卡片状态写当前项 `index/total · name`，预检阶段写 `Checking dependencies index/total`。中途冒出的依赖阻塞并进依赖卡片、本项计失败；单项失败（下载、校验、游戏在跑）只写日志 `Batch` 区与最终摘要，然后继续下一项，不保留失败卡片。
- 收尾：跑完清 `batch_progress` / `remote_busy`，`refresh_mods` 之后写一条摘要通知（成功 / 失败数）并把摘要写进活动日志。批量期间单条更新、依赖安装与其他批量入口都被 `install_preparing()` / `remote_busy` 挡住，避免两条流水线同时改暂存目录。

## 帧预算

- 无常驻空转：全仓库没有为动画服务的 `set_interval`。常驻轮询有三个：下载进度（120ms，见「任务中心」）与桥状态、联机控制（各 2s，见「桥状态与联机控制」）；后两者只读快照信号、值不变时不写 signal，不驱动动画。
- 循环动画都随元素卸载停止：checking / refreshing 期间的 spinner（每个 20 Hz）、可更新行的徽标呼吸（每行 10 Hz，行数极少）、任务角标脉冲（20 Hz，仅在有待处理项时存在）、下载缺 Content-Length 时的进度条脉冲（20 Hz，仅该卡存在时）。
- 一次性入场动画不占稳态帧；Motion preset 自带 `reduce_motion` 处理（无位移、直接到终态）。

## 界面文案与多语言

- 文案全部来自仓库根目录的 `locales/app.yml`（与 `nsh.json` 同级），由 nsh 在 `nsh run` / `nsh dev` / `nsh test` / `nsh package` 时读取、写成 `.nsh/rust/translations.json` 并注入生成的入口。内置三种语言：`en`、`zh-CN`、`ja`；顶层必须写 `_version: 2`，**格式是 key 在外、语言在内**（rust-i18n 的 `_version: 2` 写法）：
  ```yaml
  _version: 2
  nav.settings:
    en: Settings
    zh-CN: 设置
    ja: 設定
  ```
  每个 key 下三种语言都要有；缺键时 `t!` 直接渲染 `<locale>.<key>`。
- 取文案一律用 `nsh_core::prelude::t!`：`t!("key")`、`t!("key", name = expr)`，占位符写 `%{name}`。**`t!` 必须写在显示处**——`text(move || t!("…"))`、`pix(move || t!("…"), …)`、`button(move || t!("…"), …)`——因为 `translate` 读取的是当前 locale 的响应式状态，只有显示闭包里的调用会在 `set_locale` 后重渲染。先把结果算成 `String` 存进 signal 或结构体字段再显示，切换语言时不会更新，只能靠 `Ui::set_language` 手动重建。动态文案（如 Enable/Disable）同样在闭包里 `match` 后调 `t!`，不要在外面拼字符串。
- 语言存在 core 的 `Settings.language`（默认 `"system"`）。`"system"` 表示跟随设备首选语言，其余存 BCP 47 标签（`en` / `ja` / `zh-CN`）；`src/i18n.rs` 里是白名单 `LANGUAGES`，解析失败按 `system` 处理。启动时 `Ui::new` 读该字段：`system` → `set_locale(None)`，否则 `Locale::parse` 成功后 `set_locale(Some(locale))`。Settings 页的 Language 段四个按钮调用 `Ui::set_language`：先 `set_locale`，再写 `settings.language` 并 `Hub::save_settings`（失败只写本地化状态行），然后复用现有缓存重建 local / nexus / remote 行——不联网、不重新扫盘，重启后从 settings 恢复。
- 限制：普通 `cargo run` / `cargo test` 不注入翻译表，`t!` 只会回退到键名或英文；要验证注入行为必须走 nsh 命令（本仓库日常仍用 cargo fmt/clippy/test，不启动 GUI）。因此不要把翻译表当作运行时可选资源加载，也不要新增 `build.rs` 去复制 locales。
- 新增语言：① 在 `app.yml` 的每个 key 下加该 locale 一行（如 `de: ...`），补齐与 `en` 相同的键；② 在 `src/i18n.rs` 的 `LANGUAGES` 加 `(code, 显示名)`——显示名用该语言自己的写法，不翻译；③ Settings 页按钮由 `LANGUAGES` 长度驱动，无需改布局；④ 设备语言为新增语言时，`system` 会自动命中。core 的 `HubError` / `ModsError` 文案与 `nsh 0.5.1 · prototype shell` 保持英文，不翻译。
## 已知边界

- nsh 的条件区域翻转即销毁，没有退出动画，因此浮层、任务卡、角标都是瞬时消失；抽屉的开合由 `sheet` 的 motion 负责，不在此列。
- 不做颜色补间：`transition_colors` 会产生调色板外的中间色。布局补间只允许 `animate_layout` 这条原生路径（目前只有侧栏折叠），不写手写定时器阶梯，也不把布局动画铺到列表行。
- 列表行未做逐行错峰入场：列表在刷新时整体重建，错峰会重复播放并干扰扫描。

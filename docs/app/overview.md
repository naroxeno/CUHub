# app 层总览（`crates/app`）

## 职责

nsh 0.5.1 桌面壳：顶栏、侧栏树形导航、状态行、七页内容区（模组树下的 Nexus / 远程模组 / 本地模组，加 Multiplayer / Skins & Backups / Hub Home / Settings）与开发者控制台浮层。数据全部来自 `cuhub_core::hub::Hub`；后台任务只写 signal，界面只读 signal（线程模型见 `docs/ARCHITECTURE.md`）。nsh 细节与坑见同目录 `nsh-notes.md`。

## 导航树

- 唯一数据源是 `src/shell/nav.rs` 的 `NAV` 表：每条含 `id`、locale `key`、窄模式 `icon`、可选 `group` 与 `NavKind`（`Page` 渲染函数 / `Action` 副作用）。加页只改表、`pages` 与 locale，不动 `shell.rs`。
- 当前 8 条：Search（动作占位）、Nexus / 远程 / 本地（`group = "mods"`）、Multiplayer、Skins & Backups、Hub Home、Settings。组头来自 `GROUPS`，只翻 `nav_closed_group`，不写 `page`。
- `Ui.page` 是 `Signal<&'static str>`，存 id；页头、状态行与开发者浮层统一用 `nav::title(id)`，组内条目自动拼「模组 / 子页」。
- 侧栏两态：宽 176px（组头 28、子项 26、缩进 16 + 1px 竖线 + 8）与窄 44px（只有图标，组间 1px `BORDER` 分隔）；开关在 `nav_collapsed`，与 `nav_closed_group` 一样只存内存。
- 页脚是启动键（`NAV` 之外的 `LAUNCH_ENTRY`，走 `Ui::launch_game`）与版本行；版本行五连点开开发者浮层。
- 搜索入口当前只写状态行提示，搜索本体与「跳转到具体条目」仍未实现。

## 视觉约束

- 固定 14 色 token（`src/palette.rs`）。不新增颜色，也不做颜色补间：任何中间色都会离开调色板。
- 所有尺寸按 `zoom`（1x/2x/3x，默认 2x）缩放，统一走 `ui::scaled` / `ui::pix` / `ui::panel` / `ui::button`；动画只作用于样式层（opacity、位移），不参与布局，也不写死像素。
- 像素风的"fancy"限定为收放、脉冲、闪烁、滑动、逐行出现，不用毛玻璃、渐变、弹跳堆料。

## 动画机制

组件只运行一次，因此动画一律由 nsh 原生声明式 API 驱动，不用 `set_interval` 轮询：

- `motion().entering(MotionPreset::…)`：挂载即播放一次。绑在 `when` / `each` 区域内的元素上，区域销毁时动画随之结束。
- `KeyframeAnimation` + `Keyframes::Pulse`：只在该元素存在期间跑，`frame_rate` 显式压低帧率上限。
- `effect`：signal 变化后触发一次播放，不额外起定时器。

| 位置 | 触发 | 效果 | 时长 / 帧率 |
| --- | --- | --- | --- |
| 七个页面根视图 | 切页（`when` 重新挂载） | 淡入 + 自下而上 24pt 滑入 | 450ms，一次性 |
| 侧栏选中项标签 | 该项被选中（`effect` 观察 `page`） | 标签淡入点亮 | 300ms，一次性 |
| 状态行 | `notice` 变为非空 | 提示自左滑入 24pt + 淡入 | 300ms，一次性 |
| 开发者控制台 | 打开浮层 | 遮罩淡入 + 面板滑入 | 300ms / 450ms |
| 删除确认弹窗 | 打开（`when` 重新挂载） | 遮罩淡入 + 面板滑入 | 300ms / 450ms |
| 设置向导 | 打开 / 切步 | 遮罩淡入 + 卡片滑入；步骤内容淡入 | 450ms / 200ms |
| PENDING UPDATE 面板 | 待定计划出现（本地模组页与远程模组页共用） | 滑入 | 450ms |
| Check 进行中 | `can_check` 为假且已知仓库 | 4 个 3px 方块脉冲追逐 | 640ms 循环，20 Hz 上限 |
| 可更新行的 State 徽标 | `row.can_update()` | WARN 色呼吸 | 1800ms 循环，10 Hz 上限 |
| Nexus 刷新中 | `nexus_refreshing` | 同款方块 spinner + 文案 | 640ms 循环，20 Hz 上限 |
| 行 hover / 侧栏按下 | 指针 | 底色 `surface_raised`、底部分隔线转白；导航按下转 `border_dim` | 即时 |

## 模组行布局

- 证据行与来源行的说明文字放在 `flex_1` + `min_w(0)` 容器里并截断，行内按钮 `flex_shrink_0`。窗口较窄或来源说明很长时先裁掉文字，按钮不会被挤出面板（修掉过 Disable 因按钮在窗口外而点不到的回归）。

## 远程模组页

- 数据来自 `Hub::sources()`（core 启动时装载），页面不联网；行模型在 `src/ui/remote.rs`，与已装模组按 matcher（guid / dll / name）关联，同优先级取 guid 最优。
- 状态都在 `Ui` 上：`remote_all` / `remote_rows` / `remote_page` / `remote_total` / `remote_query` / `remote_filter` / `remote_busy`；搜索、过滤与翻页只重建内存切片，不写设置。
- 安装与更新共用 `pending_install`：准备阶段写 `remote_busy`（该行按钮与其它行一起禁用），成功后进 PENDING UPDATE 面板，Apply 走既有的 `apply_pending`，Cancel 删 staging；`refresh_mods` 同时重建本地与远程行，装完按钮立即变成 Update。
- 空态分三种：清单缺失（一行说明 + 清单路径小字）、清单无条目、过滤后为空。

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

## 帧预算

- 无常驻空转：全仓库没有为动画服务的 `set_interval`。
- 循环动画只有两类，且都随元素卸载停止：checking / refreshing 期间的 spinner（每个 20 Hz），以及可更新行的徽标呼吸（每行 10 Hz，行数极少）。
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

- nsh 的条件区域翻转即销毁，没有退出动画，因此浮层、pending 面板都是瞬时消失。
- 不做颜色/布局补间：`transition_colors` 会产生调色板外的中间色。
- 列表行未做逐行错峰入场：列表在刷新时整体重建，错峰会重复播放并干扰扫描。
- `docs/FEATURES.md` 的 O10 仍写着"无补间、无淡入淡出"，与上述实现不一致；功能清单的正式措辞待开发者决定后再改，本层不擅自改写。

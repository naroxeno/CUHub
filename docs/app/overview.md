# app 层总览（`crates/app`）

## 职责

nsh 0.5.1 桌面壳：顶栏、侧栏导航、状态行、六页内容区（Nexus / Local Mods / Multiplayer / Skins & Backups / Hub Home / Settings）与开发者控制台浮层。数据全部来自 `cuhub_core::hub::Hub`；后台任务只写 signal，界面只读 signal（线程模型见 `docs/ARCHITECTURE.md`）。nsh 细节与坑见同目录 `nsh-notes.md`。

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
| 六个页面根视图 | 切页（`when` 重新挂载） | 淡入 + 自下而上 24pt 滑入 | 450ms，一次性 |
| 侧栏选中项标签 | 该项被选中（`effect` 观察 `page`） | 标签淡入点亮 | 300ms，一次性 |
| 状态行 | `notice` 变为非空 | 提示自左滑入 24pt + 淡入 | 300ms，一次性 |
| 开发者控制台 | 打开浮层 | 遮罩淡入 + 面板滑入 | 300ms / 450ms |
| PENDING UPDATE 面板 | 待定计划出现 | 滑入 | 450ms |
| Check 进行中 | `can_check` 为假且已知仓库 | 4 个 3px 方块脉冲追逐 | 640ms 循环，20 Hz 上限 |
| 可更新行的 State 徽标 | `row.can_update()` | WARN 色呼吸 | 1800ms 循环，10 Hz 上限 |
| Nexus 刷新中 | `nexus_refreshing` | 同款方块 spinner + 文案 | 640ms 循环，20 Hz 上限 |
| 行 hover / 侧栏按下 | 指针 | 底色 `surface_raised`、底部分隔线转白；导航按下转 `border_dim` | 即时 |

## 模组启用/禁用

- 行内 Toggle：禁用行显示 `Enable`、启用行显示 `Disable`。点击后在后台线程调用 `Hub::set_mod_enabled`，成功后重新扫描并把改名的 DLL 数写进状态行；失败只把 `HubError` 文案写进状态行，不弹窗。
- 面板批量：`Disable all` / `Enable all` 调 `Hub::set_all_mods_enabled`，同样后台执行并回写改名数。任一变更进行期间，行内 Toggle 与两个批量按钮一起置灰，避免重复点击。
- 禁用行视觉（FEATURES E7）：左缘 2px danger 竖条由 `status_color` 提供；名称、版本、作者、证据与详情文字转 danger，来源行在带 warning 时仍保留 WARN，避免丢掉来源匹配提示。
- 行 key 里包含 `disabled`，刷新后 `each` 会重建该行，静态文本与按钮才能跟随状态变化。
- 两项都是可逆操作，不做确认弹窗；Delete all 与单项 Delete 属破坏性操作，界面暂未实现，等开发者确认。

## 帧预算

- 无常驻空转：全仓库没有为动画服务的 `set_interval`。
- 循环动画只有两类，且都随元素卸载停止：checking / refreshing 期间的 spinner（每个 20 Hz），以及可更新行的徽标呼吸（每行 10 Hz，行数极少）。
- 一次性入场动画不占稳态帧；Motion preset 自带 `reduce_motion` 处理（无位移、直接到终态）。

## 已知边界

- nsh 的条件区域翻转即销毁，没有退出动画，因此浮层、pending 面板都是瞬时消失。
- 不做颜色/布局补间：`transition_colors` 会产生调色板外的中间色。
- 列表行未做逐行错峰入场：列表在刷新时整体重建，错峰会重复播放并干扰扫描。
- `docs/FEATURES.md` 的 O10 仍写着"无补间、无淡入淡出"，与上述实现不一致；功能清单的正式措辞待开发者决定后再改，本层不擅自改写。

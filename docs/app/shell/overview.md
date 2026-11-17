# shell

## 职责

包住内容区的外壳：标题栏、侧栏、状态行、开发者浮层，以及按当前页挂载内容的滚动容器。不持有业务状态，所有数据来自 `ui::Ui` 的 signal；页面函数放 `pages`。

## 结构

| 文件 | 职责 |
| --- | --- |
| `shell.rs` | 模块根；`content()` 遍历导航表生成 `when` 区域 |
| `shell/nav.rs` | 导航数据的唯一来源：id、locale key、图标、分组、页面/动作 |
| `shell/sidebar.rs` | 宽树与窄图标栏、折叠开关、组头、页脚启动键与版本行 |
| `shell/title_bar.rs` | 标题栏：居中的产品名与版本，不放操作入口 |
| `shell/statusline.rs` | 状态行：notice 优先，否则「页面标题 · zoom」 |
| `shell/dev.rs` | 五连点版本行打开的开发者浮层 |

## 导航模型

- `NavKind::Page(fn(&Ui) -> AnyElement)` 进内容区；`NavKind::Action(fn(&Ui))` 只执行副作用（页脚启动）。
- `group: Some("mods")` 的条目在宽模式归到 `GROUPS` 的组头下；`page` 存的是 id，不是索引，重排不影响页头。
- `title(id)` 对组内条目返回「父 / 子」，其余返回自身 key；找不到时回退 settings。
- 组头只翻 `nav_closed_group`，不写 `page`；组内页面被折叠时仍保持选中。

## 渲染规则

- 宽 176px：组头 28、子项 26、缩进 16 + 1px 竖线 + 8；选中项 2px ACCENT 左描边与 SURFACE_RAISED 底。
- 窄 44px：只画图标，不画组头；组边界用 1px BORDER 分隔；选中同样有左描边。
- 页脚：启动键始终显示，版本行只在宽模式；版本行五连点在 3 秒窗口内计数。
- 折叠状态、组开关都在 `Ui` 的内存 signal，不落盘。
- 动画只用 `motion()`：切页时选中标签 FadeIn；不新增常驻定时器。

## 边界与失败模式

- 侧栏不读 Hub、不联网、不写盘。
- `NAV` 是 `static`，条目顺序就是侧栏顺序；新增页面必须同时补 `pages` 函数与 locale key，否则 `t!` 会渲染键名。

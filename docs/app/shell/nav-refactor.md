# 侧栏与 shell 重构（已实施 2026-11-17）

## 结果

页面身份不再散落：侧栏与内容区都读 `shell/nav.rs` 的 `NAV` 表，加一页只需在表里加一行、在 `pages` 里加函数、补 locale key。

- `shell.rs` 模块根：`mod dev/nav/sidebar/statusline/top_bar` + `pub use`；`content()` 遍历 `NAV` 的 `NavKind::Page` 条目生成 `when` 区域。
- `shell/nav.rs`：`NAV`（8 条）、`GROUPS`（模组组）、`NavEntry`、`NavKind { Page(fn(&Ui) -> AnyElement), Action(fn(&Ui)) }`、`title(id)`、`in_group`。
- `shell/sidebar.rs`：宽 176px（组头 28、子项 26、缩进 16+竖线+8）与窄 44px（图标行、组边界 1px BORDER）两态，页脚启动键 + 版本行（仅宽）；窄模式不分组建，图标取自 `NavEntry::icon`。
- `Ui.page: Signal<&'static str>`（默认 `nav::NEXUS`），新增 `nav_collapsed`、`nav_closed_group`（空串 = 都展开），删除 `mods_menu_open`；两者都在内存，不落盘。
- `i18n.rs` 删掉 `PAGE_KEYS`、`NEXUS_PAGE`/`REMOTE_PAGE`/`LOCAL_PAGE`、`MODS_PAGE_COUNT`、`page_label`；页头、状态行、开发者浮层统一用 `nav::title(id)`，组内条目自动拼「模组 / 子页」。
- 搜索入口是 `Action`，当前只写状态行 `notice.search_soon`；搜索本体仍留第二期，需要给 `Ui` 加 `focus` 通道才能跳到具体条目。

## 加一个页面

1. `shell/nav.rs` 加 id 常量与 `NAV` 条目（`Page` 或 `Action`，可选 `group`）。
2. `pages` 加渲染函数，并在 `nav.rs` 的转发函数里包一层 `into_element()`。
3. `locales/app.yml` 补 `nav.<id>` 三语；`NavKind::Page` 会自动进内容区，不需要改 `shell.rs`。

## 未做

- 窄模式没有悬停名称（nsh 的 tooltip 原语未确认）；没有折叠状态持久化。
- 搜索、启动键复用顶栏同一个 `Ui::launch_game`，两处入口并存。

## 审计发现（app 其余部分，2026-11-17 状态）

1. 渲染期锁 Hub：已修，`Ui.sources_ready` 在构造时算好。
2. 页面索引硬编码：已随 id 化清除。
3. `nav_item`/`nav_child` 重复与写死分组：已合并为 `nav_row`，组来自 `GROUPS`。
4. 版本号硬编码：已改 `env!("CARGO_PKG_VERSION")`。
5. 无上限轮询：已加 60 次（约 3 分钟）上限并提示 `notice.game_still_running`。
6. UI 线程写盘（Nexus key、GitHub token、来源 pin、setup 扫盘）：未动，仍待办。
7. 占位 UI（假房间码、禁用按钮）：未动，按 FEATURES R 的样例数据保留。
8. `NexusModRow::row_key` 含状态导致整表重建：已知，暂不处理。

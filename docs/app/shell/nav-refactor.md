# 侧栏与 shell 重构方案（提案，未实施）

## 现状

页面身份散在六处：`i18n.rs` 的 `PAGE_KEYS`、`MODS_PAGE_COUNT`、`page_label` 的条件，`shell.rs` 的 `skip/take`、`content()` 手写的七个 `page_region`、`page_for` 的 match。加一页要同时改六处，漏一处就是静默错误（页头显示成别的页）。

侧栏的数据源是「页面列表」，因此表达不了动作、分组开关、搜索这类条目；`nav_item` 与 `nav_child` 各写了一遍 55 行，`mods_header` 把唯一的分组写死。

## 目标

1. 侧栏数据 = 一张 `NAV` 表，条目分「页面」与「动作」两种。
2. 页面身份用稳定 id（`&'static str`），不用索引。
3. `shell.rs` 拆成目录，一个文件一件事。
4. 侧栏支持折叠（图标模式）、搜索入口、底部启动键。

## 结构

```
crates/app/src/shell.rs          模块根：mod 声明 + pub use
crates/app/src/shell/nav.rs      NAV、GROUPS、NavEntry、NavKind、title(id)
crates/app/src/shell/sidebar.rs  宽 / 窄两种渲染
crates/app/src/shell/top_bar.rs
crates/app/src/shell/statusline.rs
crates/app/src/shell/dev.rs      开发者浮层
crates/app/src/shell/search.rs   第二期
```

## 模型

```rust
pub enum NavKind {
    Page,
    Action(fn(&Ui)),
}

pub struct NavEntry {
    pub id: &'static str,            // "nexus"、"launch"
    pub key: &'static str,           // "nav.nexus_dashboard"
    pub icon: &'static str,          // 窄模式字形
    pub group: Option<&'static str>, // Some("mods")
    pub kind: NavKind,
}

pub struct NavGroup {
    pub id: &'static str,
    pub key: &'static str,
    pub icon: &'static str,
}
```

- `Ui.page: Signal<usize>` 改成 `Signal<&'static str>`，存 id。
- `i18n::page_label(index)` 改成 `nav::title(id)`；页面文件不再写 `page_label(0/1/2)`，改用自己那行的 id 常量。
- 组头来自 `GROUPS`，点击只翻转该组的开关，不改 `page`。

## 渲染规则

- 宽：组头 + 子项缩进；组头可折叠，折叠状态按组 id 存在 `Ui`。
- 窄：不渲染组头，图标一行一个，组与组之间画一条 `BORDER` 亮一档的分隔线；点图标直接跳转。
- 页脚：启动键（`Action`）与版本行。
- 窄模式没有文字，悬停名称需要单独的浮层，列为后续项。

## 迁移（三个原子提交）

1. 引入 `nav.rs` 与 `NAV` 表，删掉 `PAGE_KEYS`、`MODS_PAGE_COUNT`、`page_for`、手写 `page_region`，页面 id 化。行为不变，先拿到同构。
2. `shell` 拆目录，`nav_item`/`nav_child` 合并成一个按层级渲染的函数。
3. 侧栏新功能：折叠、图标、组分隔、底部启动键、版本行改用 `CARGO_PKG_VERSION`；搜索入口留空壳。

搜索本体（跨页面、模组、目录）放第二期，需要先给 `Ui` 加一个 `focus` 通道，用来从搜索结果跳到具体条目。

## 风险

- `page` 的类型变化会波及 `pages/*.rs` 里所有 `panel(...)` 标题与 `dev` 浮层，属于机械替换，但面广。
- 折叠状态不持久化，重启回到展开；进 `settings` 是后续的事。
- 窄模式的悬停名称需要 nsh 有没有 tooltip 原语，未确认。

## 验收

- 行为与当前一致（新增折叠除外），页头路径随 id 正确。
- `cargo fmt --check`、`clippy -D warnings`、`cargo test` 干净。
- 模块文档 `docs/app/shell/overview.md` 与架构文档模块表同步。

## 审计发现（app 其余部分）

按严重程度排：

1. **渲染期锁 Hub**：`pages/remote.rs:26` 调 `ui.sources_present()` → `hub().sources()`，在渲染路径上取互斥锁。后台下载期间锁被占着，UI 线程会一起停住。应改为刷新时算好存进 `Signal<bool>`。
2. **索引硬编码**：`pages/remote.rs:88`、`local.rs:39`、`nexus.rs:42` 分别写死 `page_label(1/2/0)`，重排即错。与本次 id 化同批修。
3. **重复代码**：`nav_item` 与 `nav_child` 几乎逐行重复；`mods_header` 把唯一分组写死。
4. **版本号硬编码**：`shell.rs:29` 的 "0.1.0"、`shell.rs:258` 的 "v0.1.0"，应读 `env!("CARGO_PKG_VERSION")`。
5. **无上限轮询**：`install.rs:173-176` 游戏运行中每 3 秒重试一次，游戏不关就永远循环，`apply_running` 一直为真。取消路径存在，但缺一个超时或提示，用户容易当成卡死。
6. **UI 线程写盘**：`save_nexus_key`、`save_github_token`、`cycle_source_pin` 在事件处理里直接落盘；目录大时 setup 里的 `refresh_mods` 也会同步扫盘。都能移到 worker。
7. **占位 UI 像真的**：`pages/misc.rs` 的联机页有 "CUHB-0000-0000" 房间码和两个禁用按钮，皮肤页同理。要么标 experimental，要么先隐藏。
8. **列表 key 含状态**：`NexusModRow::row_key` 含 `status`/`version`，状态一变整表重建，滚动位置与焦点会跳。可接受但要知道。

未发现的问题：app crate 里没有 `unwrap()`/`expect()`；`Mutex` 毒化用 `into_inner` 统一兜住；后台任务都走 `spawn_worker` 挂在 app owner 上。

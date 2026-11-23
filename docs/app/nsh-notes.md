# nsh 使用笔记（0.5.1）

给下一位接手者：nsh 公开文档偏薄，很多页只讲概念；真正的 API 以 crate 源码为准。这里只记 spike 里实际用过、核对过的东西，避免重新翻一遍。

## 资源

- CLI：`~/.local/bin/nsh` → `~/.nsh/bin/nsh`，`nsh 0.5.1 (224d9a7)`。
- 文档索引：https://nsh.dev/docs/llms.txt（264 页）；单页是扁平的 https://nsh.dev/docs/<slug>.md，例如 `networking.md`、`sandbox-and-capabilities.md`、`lists.md`、`images-and-assets.md`、`testing-and-trace.md`、`packaging.md`、`rust.md`。
- crate 源码：`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/nsh-{core,base,engine,platform-linux,macros}-0.5.1/`；`nsh-core/src/prelude.rs` 与 `element/mod.rs` 是导出总表。
- 文档查不到的 API（`ResizeMode`、`WindowRef`、`ImageSrc`、`virtual_list` 细节）直接看源码，比猜快。

## 工程骨架

- `nsh.json`：`name` / `id` / `version` / `build` / `rust`（crate 目录）/ `root`（根组件名）/ `capabilities`（可选）。
- `src/lib.rs`：`#[component] pub fn App() -> impl IntoElement`；`#[native_module("...")] fn native(registry)` 里 `registry.assets(assets!())` + `registry.register::<App>()`。
- `src/main.rs`：按平台 `use nsh_platform_linux as platform;`，`platform::run_app(platform::AppSource::Rust(Root::of::<App>()), platform::AppOptions { title, app_id, size, ..Default::default() })`；`cargo run` 直接跑。
- `assets/`：`asset!("fonts/x.ttf")` 编译期读入；`assets!()` 把整个目录随 app 发布。字体 `FontAsset { uri, family }`，family 来自字体 name table。Fusion Pixel 使用官方 12px proportional zh_hans TTF；此前的 OTF 中“装”字映射到了空轮廓，不能用于界面。
- `nsh run` 是调试 runner（改代码重建重启）；另有 `nsh check` / `nsh test` / `nsh build` / `nsh package`。

## 渲染模型

- 组件只运行一次；动态内容靠 signal 闭包或 `when` / `each` / `virtual_list` 区域，不要按 React 的「每次重渲染」写。
- 样式是 typed builder：`flex_row()` / `flex_col()` / `gap_2()` / `p(px(8.0))` / `size(px(40.0))` / `w_full()` / `bg(rgb(0x...))` / `text_color()` / `text_size(px(12.0))` / `font_family(...)` / `border_1()` / `border_color()` / `rounded` / `object_fit(...)` / `active(|s| ...)` / `on_press(...)`。
- 颜色 `rgb(0xRRGGBB)`、`rgba(0xAARRGGBB)`；长度 `px(f32)`；`flex_wrap()` 不带参数。
- `when(cond, || ...)` 可 `.otherwise(|| ...)`；条件翻转时区域创建/销毁。

## 列表与浮层

- `virtual_list(count, |at| ...)`：原生虚拟化，只创建屏幕附近的行，`at.index()` 是行号。方法：`.keys(...)` / `.header(...)` / `.footer(...)` / `.empty(...)` / `.separator(...)` / `.initial_scroll_index(n)` / `.on_viewable_items_change(...)` / `.estimated_item_size(px(52.0))`。
- `overlay()`：放在根视图里，子元素按 fixed 铺满根；子元素必须用 `.absolute().inset(px(0.0))`。
- 对话框在 `nsh_base`：`alert_dialog()` 返回 `AlertDialogOptions`（`.title` / `.description` / `.cancel(id, label)` / `.destructive(id, label)` / `.action(id, label)`），`.show().await` 得到 `Result<AlertDialogResult, String>`，判断 `r.action.as_deref()`。另有 `dialog()` → `Modal`，放进视图即弹出。

## 图片

- `ImageSrc::rgba(width: u32, height: u32, pixels)`：straight RGBA、行间无 padding、长度必须 `w*h*4`。
- `ResizeMode` 只有 `Cover`（默认）/ `Contain` / `Stretch` / `Center` / `ScaleDown`，**没有 nearest**；`image(src).object_fit(ResizeMode::Stretch)`。
- nsh 在后台按显示尺寸降采样并缓存，主线程只显示结果，所以像素图不要指望运行时放大清晰。

## 任务与线程

- `spawn(future)`：跑在引擎线程，可写 signal；owner 结束任务取消；`Spawned` 可 `.cancel()`。
- `background(future)`：tokio 上跑 Send future；`blocking(f)`：阻塞任务；`set_timeout` / `set_interval`。
- `run_on_main_thread(f)` → `Future<Output = Result<R, MainThreadError>>`；无主线程（headless/web）时闭包在引擎线程提交末尾执行。
- 本工程约定：后台线程不直接写 signal；事件进 `crossbeam`，UI 侧用 `run_on_main_thread` 或主循环轮询写 signal。

## 网络

- `fetch(url).await` → `Result<HttpResponse, String>`；`HttpResponse`：`status() -> u16`、`ok() -> bool`、`header(name)`、`text()`、`body`。`HttpRequest::post` / `HttpRequest::new(method, url)`。
- Rust 侧只给整包响应，没有 TSX 的流式接口。
- `nsh.json` 的 `capabilities.network.hosts` 覆盖 fetch 的每一跳（含重定向）；**不写 `capabilities` 的独立 app 默认全放行**。要收紧时再写 hosts / http 规则。
- 远程图片、视频、声音也走 network 能力。

## 窗口与标题栏

- `current_window()` → `WindowRef`、`title_bar()` → `Edges<Points>` 在 `nsh_core::element::desktop`，不在 prelude。
- `WindowRef::set(attrs)` 传 `WindowAttributes`（全 Option，只改给出的字段）：`attrs.title_bar = Some(TitleBarOptions { style: TitleBarStyle::Hidden, height: Some(px(34.0)), ..Default::default() })`。
- 自绘标题栏：视图 `.window_drag(true)` 负责拖拽，子元素 `.window_control(WindowControl::{Minimize, Maximize, Close})` 做按钮；`WindowRef::minimize()` / `restore()` / `close()` / `set_mode()`。
- 平铺合成器（Hyprland）会忽略请求的窗口尺寸；自绘标题栏要在浮动窗口下验证。

## 缩放、全窗层与滚动

- 没有 app 级 UI 缩放：`set_zoom` 是滚动视图的缩放，`font_scale()` 只影响文字，`REM_SIZE` 固定 16pt；DIP→物理像素只由 OS scale_factor 决定。整数倍缩放只能在 app 侧把每个尺寸乘 zoom（本仓库 `ui::scaled`），切换即时重排已验证。
- `overlay()` 里放 `view().absolute().inset(px(0.0))` 不会铺满：子层塌成内容大小并贴在左上角。全窗遮罩改成根视图末尾的普通 `absolute` 子层，已验证（开发者控制台）；需要原生 Escape/backdrop 时用 `nsh_base::dialog() -> Modal`，`on_cancel` 接关闭。
- 内容超过窗口时用 `scroll()` 包一层（默认纵向，`horizontal()` 换向）；`virtual_list` 自带滚动，不要双套。

## 测试与打包

- headless：`nsh-testing` + `nsh_engine::{EngineConfig, Root}`；`Headless::start(config)`、`app.type_text(id, text)`、`app.press(selector)`、`app.wait_for(selector)`、`app.screen().texts()`；在 `cargo test` 里跑，不需要显示器。CLI 的 `--trace` 打印原生操作与耗时。
- `nsh package --target linux` 默认产出 `.nsh/out/<Name>/`（可执行文件、`nsh/bundle.nshc`、`nsh/assets`、`nsh/app.json`、`.desktop`、PNG、`.tar.gz`）；Windows 另出 zip 和 Inno Setup 安装器。`just package` / `package-*` 传 `--out dist/<platform>`，产物落在仓库根 `dist/linux|windows|macos/`。
- `just package-windows` 在 Windows 下以 Niubash 的 `niu` 执行配方与 `scripts/windows-installer.sh`；缺少 Unix `zip` / Python 时分别回退到 PowerShell 的 `Compress-Archive` 与 `scripts/patch-windows-installer.ps1`，并从当前用户的 Inno Setup 7 默认安装目录寻找 `ISCC.exe`。脚本会把 `LICENSE`、`README.md`、`README.zh-CN.md` 与 `docs/third-party/terracotta.md` 放进 stage，重打 `Cuhub.zip`，再给 `.iss` 补 publisher / `LicenseFile` / 升级与卸载清理，最后重编译 `Cuhub-setup.exe`。不要直接分发 `nsh package` 生成的原始 setup，它缺这些文件。
- 打包要 release runner，Linux 运行需要 GTK 4.14+ 与 GStreamer，构建还要 clang；Windows 打包额外需要 `zip`、`python3` 和 Wine 下的 Inno Setup 6（`iscc`）。
- Windows 安装器已在 Wine 隔离前缀验证：安装树含 exe、字体、LICENSE、README 与 docs；卸载不留文件；重装会清掉已知旧文件。原生 Windows 仍需实机确认。安装器界面仍是英文（Inno 未带 `ChineseSimplified.isl`）。
- 单文件打包与体积还没测；Windows 交叉编译已跑通。

## 编译坑（实际踩过）

- `ResizeMode::Fill` 不存在，用 `Stretch`。
- `flex_wrap` 不带参数。
- `fetch(...).await` 是 `Result`，不能直接 `.status()`。
- `os()` 返回枚举，无 `Display`，用 `{:?}`。
- 想接闭包的 helper 参数要用 `impl IntoReactive<SharedString>`，不是 `impl Into<SharedString>`。
- 返回 `impl IntoElement` 的 helper 不能再链 `.on_press`；把 handler 作为参数传进去。
- `window_size()` 不存在；需要尺寸时用 `pixel_ratio()` 或窗口 state。
- `title_bar()` / `current_window()` / `ImageSrc` / `WindowRef` 不在 prelude，按上面的路径 import。
- GTK 主题警告（`gtk.css:1:1-52`）和 `Gsk-WARNING ... gl` 是环境噪音，不影响运行。

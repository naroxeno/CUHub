# 架构

Casualties Hub 的重写：Rust 桌面应用，界面用 nsh，功能与上游 0.0.9 对齐（清单见 `docs/FEATURES.md`）。
本文件是所有架构决定的单一来源；改架构先改这里。

## 1. 边界

- 两个 crate：`crates/core`（领域逻辑，不依赖任何 UI）与 `crates/app`（nsh 界面）。`app` 不联网；所有 HTTP 与下载只经 `core::network`。
- 依赖方向只有 `app → core`；`core` 不引用 `nsh`、不读窗口状态、不改界面信号。
- 平台差异在 `core/src/platform/` 收口：`windows.rs` / `linux.rs` 导出同名函数（appdata 根、本地下载目录默认值、打开 URL、Steam 回退、注册表、卷影提示），调用方不写 `cfg` 分支。

## 2. 核心门面（方案 C）

`core::Hub` 是唯一入口，持有 paths、settings、network、catalogs、logs：

- 本地快操作（列目录、读设置、算更新状态）同步返回。
- 网络与解包等耗时操作为 async，由调用方 await；UI 不阻塞。
- 只对「`Hub` 方法返回之后才发生的状态变化」发事件，事件最多 6 个变体：

```
HubEvent::ArchiveImported   // 收件箱装完一个包，本地模组列表需刷新
HubEvent::ModsChanged       // 启用/禁用/删除后
HubEvent::IconFetched       // 封面落盘完成
HubEvent::AnnouncementRefreshed
HubEvent::Status            // 外壳状态栏文案
HubEvent::Bridge            // 桥：连接/断开、游戏状态、命令结果
```

规则：能由返回值表达的，不用事件；不引入事件总线、不引入插件系统。

## 3. 数据流

```
用户操作 → app 页面方法 → Hub::xxx().await → 结果/错误
                                   ↓ 落盘（appdata）
                          HubEvent → 外壳更新信号 → 重绘相关组件
```

- 页面在进入时重建自己的状态，读最新设置（等价于「切页即刷新」），不存在全局可变 UI 单例。
- 破坏性操作（删全部、还原、覆盖）固定两段式：先返回计划让页面弹确认，确认后才调用真正执行的方法。

## 4. 线程与异步

- UI 只在主线程；`core` 不知道主线程是谁，也不主动推 UI，只返回结果或发事件。
- 阻塞工作（解压、哈希、PNG 合成、.NET 元数据、目录全扫）走阻塞任务池，不占 UI 线程。
- 文件监视（`notify`）跑在自己的线程，只做「判定完成 → 投递事件」，不碰界面。
- 桥（`bridge`）有独立监听线程与每连接读线程：只做「解帧 → 校验 → 投递」，不落盘、不读设置；命令执行回到 `Hub` 的既有路径。
- 后台线程不碰 UI 状态：`core` 只返回结果或经 `crossbeam` 通道发事件；`app` 在主线程用 nsh 的 `run_on_main_thread` 或主循环轮询把事件写进 signal。`spawn` 的 future 本身跑在引擎线程，可以直接写 signal。

## 5. 外部系统边界

| 系统 | 用途 | 约束 |
|---|---|---|
| 社区元数据 `jimmyking9999999/Metadata-generator`（GitHub raw） | Nexus Dashboard 目录 | 6 小时缓存 + ETag/Last-Modified；失败退回离线缓存；数据可能不全 |
| Hub 配置 `casualties-hub.github.io` | 公告、外链、发布信息 | 按预发布/正式选频道；只接受 https；30 分钟间隔 |
| Nexus API v1 | 用户自带 Premium key 的直连下载 | 只有用户自己填了 key 才请求；不伪造 URL、不复用抓包链接 |
| `api.github.com` | 已安装模组的 release tag 与资产名 | 未认证 60 次/小时；6 小时缓存 + ETag；只取 tag/资产名，不自动安装 |
| 用户来源表里的任意 https 页面 | 自定义来源的版本与下载地址提取 | 只 GET、不带凭据、响应上限 2 MB、拒绝 `nexusmods.com`，失败只报错不重试风暴 |
| Steam | 启动游戏（`steam://rungameid`）、库与 appid 发现 | 必须走 Steam，直接跑 exe 在 Proton 下不加载 BepInEx |
| 游戏进程（Wine/Proton、CrossOver、Windows 原生） | 桥：运行时状态、受限命令 | 只听 `127.0.0.1` + token；游戏侧输入按不可信处理；凭据不得过桥 |
| 游戏目录 `BepInEx/Plugins`、`CustomSprites` | 安装、开关、删除、备份、皮肤 | 所有写入先解析真实大小写再校验在允许根内 |
| 系统桌面 | 打开 URL/文件夹、剪贴板 | scheme 白名单 http/https/steam |

以上远端访问只允许发生在 `core::network`；`app` 不调用 nsh `fetch`，也不加载远程图片、视频或声音。远程封面由 core 下载到 `IconCache`，app 只读本地文件。

远端响应一律不可信：链接校验 scheme，路径校验越界，数值校验范围。

## 6. core 模块

| 模块 | 职责 |
|---|---|
| `paths` | appdata 根、XDG 下载目录、大小写不敏感解析、`is_inside` |
| `settings` | 设置模型、迁移、容错读写（解析失败不覆盖原文件） |
| `network` | 唯一联网层：HTTPS 校验、条件请求（ETag / If-Modified-Since）、重定向/超时/UA、响应上限、下载落盘 |
| `metadata` | 社区目录缓存、按 DLL 名匹配模组（经 `network` 拉取） |
| `hubconfig` | 公告频道、缓存、https 过滤（经 `network` 拉取） |
| `nexus_auth` | Nexus key 与 GitHub token 的保存/读取/清除（AES-GCM + 0600；Windows DPAPI 待接） |
| `nexus` | 直连下载：files.json 选文件、download_link（经 `network`） |
| `install` | 压缩包 staging、安装计划、归属检查、备份/替换/回滚 |
| `mods` | 扫描 Plugins、版本证据、更新判定、启用/禁用、删除 |
| `catalogs` | 依赖与不兼容目录（内嵌 + `Data/` 覆盖） |
| `modlist` | `CUH1:` 编解码、缺失项匹配 |
| `sources` | 来源解析与优先级：`nexus` / `github` / `custom`（用户正则） |
| `sources::table` | `sources.json` 与 `Data/sources.json` 的读取校验、`CUPK1:` 整合包编解码 |
| `protect` / `backup` | 受保护资源与整目录备份的创建、还原、移除 |
| `skins` | 槽位发现、精灵查找、骨骼合成（像素精确） |
| `steam` | 库扫描、VDF 解析、appid 反查 |
| `launch` | 经 `steam://` 启动游戏与 `steam -applaunch` 回退；不直接跑 exe |
| `bridge` | 回环桥：监听、握手、帧协议、命令白名单与权限、rendezvous 文件 |
| `easytier` | 联机内核：版本 pin、下载校验缓存、安装探测、进程控制、房间码 |
| `logs` / `diagnostics` | 会话日志、崩溃报告、`--diagnostics` 自检 |

## 7. app 模块

- `shell`：侧栏、顶栏、状态栏、开发者控制台、内容滚动容器。
- `pages`：dashboard、local_mods、multiplayer、skins_backups、hub_home、settings。
- `setup`：首启设置向导（六步全窗层），只读写 Hub 设置；BepInEx 步只读。
- `ui`：字体、像素文字、面板（1px 双框 + 标题断口）、按钮与缩放 helper；组件多了再拆 `components`。
- `palette`：像素调色板 token 与图标着色（固定常量，不做运行时派生）。
- `assets`：内嵌像素字体、16×16 图标集。

## 8. 持久化

- 用户数据全在应用数据目录（布局见 `docs/FEATURES.md` §3），不在安装目录、不进仓库、不进日志。
- 可执行文件旁 `Data/` 可覆盖内嵌目录数据（依赖、不兼容、发布说明、离线公告）。
- 设置写缩进 JSON；凭据单独加密文件；其余缓存均为可丢弃文件。

## 9. spike 结论与剩余未决

2026-11-14 用 nsh 0.5.1 跑完 spike（工程在仓库外 `cuhub-spike`，7 页）。细节见 `docs/app/nsh-notes.md`。

已定：

1. 长列表用 `virtual_list`：原生虚拟化，只创建屏幕附近的行；详情/弹层用 `overlay()`，不自己拼 Z 序。300 行实测正常；切页与浮层之后的滚动位置保持仍待确认。
2. 后台任务不直接写 UI 状态：worker 只发事件（`crossbeam`），`app` 用 `run_on_main_thread` 或主循环 `set_interval` 轮询写入 signal。nsh 提供该 API；无主线程时闭包在引擎线程提交末尾执行。
3. 内存位图用 `ImageSrc::rgba(w, h, straight RGBA)`，`image(...)` 显示。nsh 按显示尺寸降采样并缓存，**没有 nearest/点采样开关**；像素图放大一律预生成整数倍资源，不依赖运行时缩放（皮肤/图标 M8 前落实）。
4. 网络：全部在 `core::network`（`reqwest`），由 core 的 worker 调用；`app` 不联网，不使用 nsh `fetch`，也不加载远程图片。ETag、超时、UA 与下载落盘由 core 控制，app 只读本地缓存。
5. 字体用 `asset!("fonts/…")` + `registry.assets(assets!())` 打进 bundle，family 取字体 name table（Fusion Pixel 是 `Fusion Pixel 12px Prop zh_hans`）。1x/2x/3x 下 1px 双框与字距已逐档核对，未发虚。
6. 缩放：nsh 没有 app 级 UI 缩放（`set_zoom` 只管滚动视图，`font_scale` 只放大文字，rem 固定 16pt）；所有尺寸在 app 侧按 1x/2x/3x 乘整数，切换即时生效。
7. 全窗遮罩：`overlay()` 内放 `absolute().inset(0)` 会塌成内容大小；改用根视图末尾的普通 `absolute` 子层（开发者控制台已验证），需要原生 Escape/backdrop 时用 `nsh_base::dialog() -> Modal`。设置向导与删除确认用同一挂载方式。
8. 内容区包一层 `scroll()`，3x 在矮窗口下可滚；`virtual_list` 自带滚动，页面改用列表后不要再外套 `scroll`。

剩余未决：

9. `reqwest` 自带 runtime 与 nsh 引擎线程是否冲突——M0 建 `core` 时先做最小验证，且 `core` 不得在 UI 线程阻塞。
10. Linux 文件对话框在无 portal 环境的行为（选文件、选目录、回收站共用此路径）。
11. 单文件打包、内嵌资源体积、Windows 交叉编译与 CI。
12. 平铺合成器会忽略请求的窗口尺寸；自绘标题栏的拖拽与控件区要在浮动窗口下验证。
13. 桥的传输层用回环 TCP 保底；AF_UNIX 在 Wine + .NET 下能否连通未验证（macOS 防火墙弹窗的备选）。macOS 端还依赖 `platform` 先有 macOS 面（appdata 根、打开 URL），目前只有 `linux.rs` / `windows.rs`。

## 10. 明确不做

- 自更新、遥测、匿名安装 ID、应用内卸载器、Supabase 订阅源。
- 插件系统、配置 DSL、事件总线、用户自定义主题。
- 通用 RPC、任意代码执行、宿主驱动的文件读写；桥只调用游戏侧声明过的命令 id。
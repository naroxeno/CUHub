# launch

## 职责

通过 Steam 启动 Casualties Unknown。Proton/Wine 下只有经过 Steam 才会加载 BepInEx，所以不直接运行 exe。只负责发起启动，不等待游戏进程，不注入参数。

## 输入输出

- `launch_game(game_root)`：先用 `steam::app_id_for_game` 反查 app id；成功则调用平台层打开 `steam://rungameid/{app_id}`，返回 app id。
- 平台层只 spawn 系统 handler，不等待进程退出；拿到子进程后立刻返回 app id。后台线程等待 handler：若退出码非零，再 spawn `steam -applaunch` 作为回退并等待它，避免僵尸进程。
- 系统 handler 连 spawn 都失败时，才同步尝试回退命令；两条命令都无法启动才返回 `CouldNotReachSteam`。
- `Hub::launch_game` 是 app 的入口：取设置里的游戏目录、记录一条活动日志、把错误原样返回。

## 边界与失败模式

- 找不到 app id：`NoAppId`，文案提示从 Steam 库手动启动；不猜测 app id，不扫描 exe。
- 两条启动路径都失败：错误里带 app id、系统 handler 的失败原因和回退命令的失败原因。
- 不检查 Steam 是否在运行；`steam://` 由系统 handler 交给 Steam 客户端。
- 非阻塞的代价：handler 成功 spawn 但随后失败时，回退发生在后台，UI 已经显示“已请求启动”，回退失败无法反馈到按钮；错误只体现在游戏没有启动。这是为避免 Steam 未启动时卡住 UI 的取舍。
- 平台差异收在 `platform::{open_url, steam_app_launch, steam_executable}`；`launch` 自身不写 `cfg`。
- Windows 的 `steam.exe` 从注册表与 Program Files 候选 Steam 根找（见 steam 模块文档）。
- macOS 没有单独的 Steam 候选根，回退命令按 Linux 处理。

## 常量出处

启动方式、回退顺序与文案见 `docs/FEATURES.md` N1–N3、N6；上游对应 `Services/GameLaunchService.cs`。
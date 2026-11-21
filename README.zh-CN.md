# CUHub

面向 [Casualties: Unknown](https://store.steampowered.com/app/4576490/) 的非官方桌面模组管理器和启动器。使用 Rust 和 [nsh](https://nsh.dev) 开发，支持 Windows 和 Linux。

[English](README.md)

## 能做什么

- **管理模组**。扫描 BepInEx 安装，启用、禁用模组并准备更新。支持 ZIP、7z、RAR 和单独的 DLL，安装前检查依赖并提供部署预览。
- **一起搜 GitHub 和 Nexus**。跨两个目录搜索，按来源或安装状态筛选，安装时选择来源。Nexus 支持用浏览器下载，也支持通过官方 API 授权下载。
- **切换模组档案**。在本地内容库保留各版本，通过档案切换启用组合。应用前会检查是否有未托管文件占用目标。磁盘故障中断部署时，可能留下已完成的部分改动。
- **配置 BepInEx 并通过 Steam 启动**。可以校验、安装或修复 BepInEx 5 LTS 和实验性的 BepInEx 6 Mono。
- **实验性联机**。创建 EasyTier 房间并分享 C1 房间码。客人可自动发现房主端口，把本地转发地址复制到 Casualties Together 内。可选的 CUHub Bridge 支持发起游戏内开房和加入。

## 开始使用

1. 在「设置」选择游戏目录，按需安装 BepInEx。
2. 在「远程模组」找模组，或导入本地归档。在任务中心核对来源、版本与安装目标后再应用。
3. 联机前安装 Casualties Together 及其依赖，再打开「多人游戏」。房主默认使用 **7790**，客人的房主端口可留空。房主地址变化后，用 CUHub 显示的新地址在游戏里重新加入。

CUHub 仍处于 Alpha 阶段。EasyTier 2.6.4 转发已在同机隔离环境验证，正式双机联机及 Bridge 仍待游戏实测。

## 从源码构建

需要稳定版 Rust、[nsh](https://nsh.dev) 0.5.1 和 `just`。Linux 构建需要 Clang，运行桌面应用需要 GTK 4.14+ 和 GStreamer。

```sh
git clone https://github.com/naroxeno/CUHub.git
cd CUHub
just check
just test
just dev
just package
```

`just package` 为当前平台打包。各平台专用命令见 [justfile](justfile)。Bridge 通过 `just package-bridge` 单独打包，构建需要 .NET SDK 和游戏的 Managed DLL。把 `CuhubBridge.zip` 放在主程序旁，再从「本地模组 → CUHub Bridge → 安装」应用。详见 [Bridge 说明](mods/cuhub-bridge/README.md)。

## 项目信息

- [功能清单](docs/FEATURES.md) · [架构](docs/ARCHITECTURE.md) · [路线图](docs/ROADMAP.md)
- [测试计划](docs/TEST-PLAN.md) · [交接说明](docs/HANDOVER.md)
- [AGPL-3.0-or-later](LICENSE) · [第三方许可说明](docs/third-party/)

CUHub 是独立社区项目。API 密钥保存在本机，应用不采集遥测。

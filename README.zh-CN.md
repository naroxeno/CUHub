# Cuhub

**Casualties: Unknown 的第三方模组管理器与启动器。** 用 Rust 和 [nsh](https://nsh.dev) 写成，界面是游戏的像素风。

[English](README.md) | [简体中文](README.zh-CN.md)

## 能做什么

- **管好已装的模组。** 扫描 `BepInEx/Plugins`，启用或停用、安全删除，并显示可用更新。
- **装包不靠猜。** 支持 ZIP、7z、RAR 和裸 DLL：先解到暂存区，列出精确的安装计划，覆盖前备份，失败可回滚。安装和更新前检查模组声明的依赖。
- **装好 BepInEx。** 安装或修复 BepInEx 5 LTS / 6 Mono，校验 SHA-256。
- **找模组。** 浏览社区维护的 Nexus 目录与 GitHub release 来源，或从本地清单安装。Nexus 下载只在你自己的 key 下走官方 API。
- **内容库与档案。** 模组只在内容库存一份，以链接方式部署进 `Plugins`；切换档案即可换整套模组。
- **联机（实验性）。** 内置基于 EasyTier 的虚拟局域网，用房间码加入，不需要账号，也不自建服务器。双机实测尚未进行。
- **看起来像游戏的一部分。** 像素界面，1x/2x/3x 整数缩放，支持 English / 简体中文 / 日本語。

## 状态

0.1.0 alpha。主要目标是 Windows，Linux 可用。目前还没有打包好的发行版，请从源码构建；配置与磁盘格式仍可能变化。

## 构建

需要 Rust stable、[nsh](https://nsh.dev) 0.5.1 和 `just`。Linux 构建还需要 clang，运行需要 GTK 4.14+ 与 GStreamer。

```sh
git clone https://github.com/naroxeno/CUHub.git
cd CUHub
just check
just test
just dev          # 运行，支持热重载
just package      # 为当前系统打包，产物在 dist/
```

`just package-windows`、`package-linux`、`package-macos` 分别构建指定平台。

## 文档

- [架构](docs/ARCHITECTURE.md)
- [功能清单](docs/FEATURES.md)
- [交接说明](docs/HANDOVER.md)

## 许可

[AGPL-3.0-or-later](LICENSE)。项目使用了第三方代码与素材，出处和许可说明见 [docs/third-party](docs/third-party/)。

## 免责声明

Cuhub 是非官方项目，与 Casualties: Unknown 的作者、Nexus Mods、BepInEx 均无隶属或背书关系。不做遥测，API 密钥只保存在本机。

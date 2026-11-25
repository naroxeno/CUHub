<p align="center"><img src="crates/app/assets/branding/cuprum-package.png" width="128" height="128" alt="Cuprum 像素铜矿图标"></p>

<h1 align="center">Cuprum</h1>

<p align="center">
  <a href="https://store.steampowered.com/app/4576490/">Casualties: Unknown</a> 的非官方模组管理器和启动器。
</p>

<p align="center">
  <strong>Alpha</strong> · Windows / Linux · <a href="README.md">English</a>
</p>

## 功能

| 功能 | 能做什么 |
| --- | --- |
| **管理模组** | 导入 ZIP、7z、RAR 或 DLL，预览安装和更新，检查依赖，启用或禁用模组。 |
| **查找模组** | 一起搜索 GitHub 和 Nexus Mods。Nexus 模组可通过浏览器或授权 API 下载。 |
| **模组档案** | 在本地保留多个模组版本，按档案切换模组组合。 |
| **BepInEx 与 Steam** | 安装或修复 BepInEx 5 LTS 或实验性的 6 Mono，通过 Steam 启动游戏。 |
| **联机 · 实验性** | 用 EasyTier 开房，通过房间码邀请好友。 |

## 快速开始

1. 在「设置」选择游戏目录，按需安装 BepInEx。
2. 浏览「远程模组」或导入本地文件，在「任务中心」确认安装内容。

联机前先装好 Casualties Together 及其依赖，再打开「多人游戏」。房主默认使用 **7790** 端口。加入者可留空房主端口，把 Cuprum 显示的地址粘贴到游戏里。地址变更后用新地址重进。双机联机与可选的 Bridge 仍待游戏实测。

## 从源码构建

需要稳定版 Rust、[nsh](https://nsh.dev) 0.5.1 和 `just`。Linux 构建需要 Clang，运行需要 GTK 4.14+ 和 GStreamer。

```sh
git clone https://github.com/naroxeno/CUHub.git
cd CUHub
just dev
just package
```

用 `just check` 和 `just test` 验证。各平台的构建命令见 [justfile](justfile)。可选的 Bridge 用 `just package-bridge` 单独打包，构建与安装步骤见 [Bridge 指南](mods/cuhub-bridge/README.md)。

## 项目

[功能清单](docs/FEATURES.md) · [架构](docs/ARCHITECTURE.md) · [路线图](docs/ROADMAP.md) · [测试计划](docs/TEST-PLAN.md)

[AGPL-3.0-or-later](LICENSE) · [第三方许可](docs/third-party/)

API 密钥保存在本机。Cuprum 不采集遥测。

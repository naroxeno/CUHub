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

### 从旧测试版升级

首次启动新版前，关闭旧版和游戏。在源码目录运行 `python3 scripts/migrate-app-data.py "<旧数据目录>"`，Windows 上把 `python3` 换成 `py -3`。传入包含 `Settings.json` 的目录。脚本会把数据移到同级的 `Cuprum`，并在旧位置保留目录链接，让原有模组路径继续有效。如果目标目录已存在，先手动移开再重试。

装过联机桥的用户，先把旧桥目录移出 BepInEx 的插件目录，再装新版桥。原来启用过游戏内自动开房和加入的用户需要重新打开该开关。联机双方升级后重新建房。

## 从源码构建

需要稳定版 Rust、[nsh](https://nsh.dev) 0.7.1 和 `just`。Linux 构建需要 Clang，运行需要 GTK 4.14+。

```sh
git clone https://github.com/naroxeno/Cuprum.git
cd Cuprum
just dev
```

Linux 发布包用 `just package-linux` 构建。Windows 的 ZIP 和安装器用 `just package-windows` 构建，此配方会补齐许可证和文档并重编译安装器。

如果要把解压后的 Linux 包加入应用菜单，先安装 Python 3，再从源码目录运行 `bash scripts/register-linux-launcher.sh "<包含 Cuprum 的目录>"`。移动解压目录后重新执行这条命令。

在 Linux 上运行 `just package-appimage`，AppImage 输出到 `dist/appimage/`。请把 `appimagetool` 加入 `PATH`，或用 `NSH_APPIMAGETOOL` 指定其路径。运行 AppImage 仍需要目标系统安装 GTK 4.14+。

用 `just check` 和 `just test` 验证。各平台的构建命令见 [justfile](justfile)。可选的 Bridge 用 `just package-bridge` 单独打包，构建与安装步骤见 [Bridge 指南](mods/cuprum-bridge/README.md)。

## 项目

[功能清单](docs/FEATURES.md) · [路线图](docs/ROADMAP.md) · [测试计划](docs/TEST-PLAN.md)

[AGPL-3.0-or-later](LICENSE) · [第三方许可](docs/third-party/)

API 密钥保存在本机。Cuprum 不采集遥测。

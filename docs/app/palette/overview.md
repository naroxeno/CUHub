# 主题

在「设置 → 外观与语言 → 主题」选择 Copper、Console、One Dark 或 Tokyo Night。选择保存在应用数据目录的 `Settings.json`，已有界面通过同一色板信号即时换色。没有可用的已保存主题时，使用 Copper。

## 自定义

在「设置 → 存储」显示的应用数据目录中创建 `Theme.toml`。下一次启动后，设置页会出现「自定义」，选中即应用。编辑 TOML 文件后需要重启读取。

```toml
[colors]
surface = "#28292A"
accent = "#A0E4BC"
```

只需填写要覆盖的色值，其余继承 Copper。可用键见 [功能清单 O1](../../FEATURES.md#o-外观系统像素风)。每个值使用六位十六进制 RGB。文件格式错误、颜色无效或键不存在时，启动状态栏显示错误，已选的自定义主题回退到 Copper。

## 色板来源

- Copper 沿用 Cuprum 的深灰与浅绿。Console 基于早期的纯黑、白和亮绿配色。
- One Dark 从 [Atom One Dark Syntax](https://github.com/atom/one-dark-syntax/blob/master/styles/colors.less) 映射语义颜色，补充桌面控件用的边框与交互底色。
- Tokyo Night 使用 [folke/tokyonight.nvim 的 Night 配色](https://github.com/folke/tokyonight.nvim/blob/main/extras/kitty/tokyonight_night.conf)，将上游颜色映射到 Cuprum 的配色角色。

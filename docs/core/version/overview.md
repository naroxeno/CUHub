# version

## 职责

Hub 的 SemVer 读取与排序（S4），供频道判定、更新比较和发布说明文件名使用。不读程序集，不做网络。

## 输入输出

- HubVersion::parse(&str)：接受可选单个 v/V 前缀、三段数字、可选 -prerelease；先丢弃首个 + 之后的内容并 trim。
- 排序：数值段按数值比较；同号正式版高于预发布；预发布逐段比较，数字段低于字母段，两边都数字按数值，否则按 ASCII 忽略大小写；段数少的一方更小。
- is_prerelease/prerelease/major/minor/patch 供 UI 与频道选择；Display 返回去掉前缀与构建元数据的原始文本。

## 边界与失败模式

- 只接受 d+.d+.d+，每段必须能放进 u32；拒绝空串、四段、非法前缀、空预发布与预发布中的非 [0-9A-Za-z.-] 字符。
- 预发布里的数字段用 u32 解析；溢出视为字母段（与上游 int.TryParse 行为接近）。上游用 int32，极端大版本号上存在差异，正常版本不受影响。
- 不提供 Current()：入口程序集版本由 app crate 在编译期注入并传入，core 不猜自己的版本。

## 常量出处

规则见 docs/FEATURES.md S4；实现对照上游 Models/HubVersion.cs。

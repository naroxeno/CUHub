# Thunderstore

远程模组页使用 Thunderstore 的 Casualties: Unknown 社区 v1 `package-listing-index`。索引列出 gzip JSON 分块的地址。客户端校验 HTTPS、已知 Thunderstore 域名与 `blob-storage/sha256` 对象目录。对象名以 SHA-256 开头、以 `.blob` 结尾，中间的存储后缀可变。非法地址只报告块序号、域名及原因。核心沿用现有 HTTP 限时、限量和下载进度，并限制分块数量和解压大小。

目录存于应用数据目录 `Cache/Thunderstore/packages.json`。分块全部读取、合并并通过校验后，才原子替换旧缓存。已有整表 JSON 缓存可继续离线浏览，安装需要获取选中版本的 ZIP。

来源记录为 `thunderstore:<namespace>-<name>`。Store 在现有目录模型内使用小写、来源限定的 `thunderstore.<namespace>-<name>` ID，版本取发布版本。包身份、插件 GUID 和 DLL 版本分别处理。

安装支持普通 Plugins 载荷。根目录 DLL 可携带同名资源子目录，Cuprum 会保留两者的相对路径并一起部署。根清单须与目录中的版本及依赖一致。包身份或插件冲突、无法取得满足精确版本的依赖，以及无法映射到 Plugins 的载荷都会阻止安装。

`BepInEx-BepInExPack-5.4.2305` 由游戏目录里的运行时满足。此包对应 BepInEx 5.4.23.5，要求完整的 Mono BepInEx 5，按 DLL 的四段装配版本判断是否达到下限。准备与确认安装时都会重新检测，BepInEx 本体继续由独立入口管理。[Thunderstore 的版本说明](https://thunderstore.io/c/casualties-unknown/p/BepInEx/BepInExPack/v/5.4.2305)。

准备下载前，依赖预览读取目录快照和当前档案的实际部署，递归展开传递依赖。结果按依赖优先排序，并指出不可用版本、循环、版本冲突和待处理条目。预览不下载 ZIP，也不写入游戏目录。所有依赖均满足时继续现有单包准备。含多个待安装项时可一次确认。确认后重新求解、校验全部归档与跨包冲突，再入 Store。部署前记录原档案、待追加条目和准备发布的副本。进程中断后可在任务中心继续同一事务，已发布项须与 Store 内容一致。全部部署通过校验后提交 Links 和档案。续装使用原游戏目录和 Store，以排他的移动操作发布准备好的文件。若目标已被占用或已发布的文件发生修改，停在该次安装并保留现场。

约定来源：[Thunderstore 服务端](https://github.com/thunderstore-io/Thunderstore) · [包格式](https://wiki.thunderstore.io/mods/creating-a-package) · [BepInEx 目录](https://wiki.thunderstore.io/mods/packaging-your-mods)。

远程模组默认展示 Thunderstore。GitHub 和 Nexus 可通过来源筛选切换。Cuprum 可把已部署的 CUCoreLib 识别为 `Jimmyking-CUCoreLib` 的本地提供者。匹配使用 DLL 中的 BepInEx GUID `net.cucorelib` 与插件版本，其中上游源码的 `1.1` 对应包版本 `1.1.0`。若 Store 已有同版本 Thunderstore 包，Cuprum 先校验 Store，再按完整部署文件清单和 SHA-256 比对本地内容。一致时可直接复用。仅 GUID 与插件版本吻合时，任务中心显示候选版本和路径，玩家须明确确认复用。确认绑定本次计划中的 DLL SHA-256。旧版中断记录里的本地路径须在续装时重新确认。版本不一致、无法确认身份或部署、以及同时检测到多个启用候选时报告路径。确认与续装时重新检查。

# room 模块

## 职责

房间码 C1 的编解码，以及由房主 hostname 在 peer 列表里认出房主。纯逻辑：不拉起进程、不发包、不碰 UI。

## 房间码格式

16 个 5 bit 字符，32 字符表 `23456789ABCDEFGHJKLMNPQRSTUVWXYZ`（无 0/O/1/I）：前 8 位网络名（`cuprum-XXXXXXXX`），中 7 位网络密钥，末位为前 15 位的 XOR。显示为 `C1-XXXX-XXXX-XXXX-XXXX`，`C1` 是协议版本，遇到 `C2+` 报版本过新而不是格式错误。

解析宽松：大小写不敏感，忽略 `-`、`/` 与空白，`C1-` 前缀可省略；校验失败与格式错误分开报，便于界面给不同提示。

## 房主发现

房主把 hostname 设为 `cuprum-host-<端口>`；客人读取 EasyTier 对端 JSON，验证规范的 hostname 并读取所通告的虚拟 IP；留空时发现唯一房主，手填时按该端口筛选。`find_host_candidates` 返回去重后的全部命中，`find_host` 只在恰好一个命中时给结果——多个房主时由调用方报错，不猜。

端口变化后 hostname 随之变化，旧候选立即失效；客人侧必须按当前端口重新匹配，不做跨端口回退。

## 边界

不判断 p2p/relay 路由状态（`EasyTierMember` 目前没有路由字段），不重试、不超时——重试节奏在 Hub。密钥只在内存里，`Debug` 只输出网络名前 4 字符。

# network

## 职责

core 里唯一允许发起 HTTP 的模块。app 不联网，也不调用 nsh 的 fetch；所有远端访问都经过这里。

## 输入输出

- Network::new()：默认 User-Agent 为 Cuhub/<core 版本>；with_user_agent 允许调用方覆盖。
- get(url, etag, last_modified, max_bytes) -> Fetch：
  - 返回 Fetched { body, etag, last_modified }，或 304 时返回 NotModified。
  - 有 ETag / Last-Modified 时自动带 If-None-Match / If-Modified-Since。
- download(url, destination, max_bytes) -> 写入字节数：
  - 先写同目录的隐藏临时文件，成功后 rename 到 destination；中途失败删除临时文件。
  - destination 已存在时直接报错，不覆盖。
- HttpState 由调用方（metadata、hubconfig）持久化；本模块只收发头部。
- is_https_url(value)：给需要校验远端链接的调用方用的只读判断，规则与请求地址一致（https、无凭据）。

## 安全与网络规则

- 只允许 https；拒绝带用户名或密码的 URL；重定向同样只允许 https、无凭据、最多 3 次。
- 连接超时 10 秒，单次请求总超时 15 秒。
- 响应体有上限：调用方给 max_bytes；超过 Content-Length 或实际读取字节数都报 TooLarge。
- 不保存 cookie，不自动重试，不跟随非 https 重定向，不把响应头写日志。
- 所有远端内容仍是不可信输入；本模块只负责传输，不解析、不执行。

## 边界与失败模式

- 不做认证、不注入 Nexus key；需要认证头的调用方在后续 nexus 模块里显式构造请求（尚未实现）。
- `Network` 的 API 是阻塞的；调用方（Hub / worker）负责把它放到阻塞池或后台线程，UI 线程不得直接调用。
- download 的临时文件与目标在同一文件系统，保证 rename 原子；Windows 上 rename 不覆盖已存在目标，因此先检查 destination.exists()。
- 网络错误不吞：InvalidUrl / InsecureUrl / UrlHasCredentials / Request / HttpStatus / TooLarge / DestinationExists / Io / Read。
- 本模块不读取设置、不决定缓存策略、不选择下载目录；这些属于调用方。

## 常量出处

超时与 UA 见 docs/FEATURES.md U8 与上游 UniversalMetadataService.cs / HubConfigService.cs；重定向上限与 https 校验见 docs/FEATURES.md U8。
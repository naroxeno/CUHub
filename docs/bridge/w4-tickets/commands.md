# 环境、取证与注入命令

真机回归用的公共命令。用例里的 `I1`–`I5` 与 `§2`/`§3` 引用都指向本文件；总览见 [README.md](README.md)，裁决见 [decisions.md](decisions.md)。

## 1. 环境与硬前置

A、B 各准备一次，结果贴进 `$EVID/versions.txt`：

1. 游戏：Steam「Casualties Unknown」能正常启动到主菜单。
2. Cuhub：同一构建（记录版本与 commit）；`nsh dev` 或发布包均可，但两端必须一致。
3. 桥插件：Cuhub「本地」页显示已安装；游戏目录存在 `BepInEx/plugins/CuhubBridge/`（`docs/bridge/plan.md:28`）。
4. CUCoreLib 与 CasualtiesMP/Together 已装好；游戏内「打开多人联机模组菜单」能打开。
5. EasyTier 核心由 Cuhub 管理（`$CUHUB_DATA/Tools/easytier/`），两端版本一致。
6. 两端网络能互通：同一公共服务器或同一局域网；Linux 防火墙放行 UDP（至少游戏端口段），`CAP_NET_ADMIN` 已授予（否则隧道直接显示「需要管理员权限」）。

```bash
export GAME="$HOME/.steam/steam/steamapps/common/Casualties Unknown"
export CUHUB_DATA="$HOME/.local/share/CUHub"
export EVID="$HOME/cuhub-w4-evidence"
mkdir -p "$EVID"
printf 'date=%s host=%s game=%s\n' "$(date -Is)" "$(hostname)" "$GAME" | tee -a "$EVID/versions.txt"
ls "$GAME/BepInEx/plugins/CuhubBridge/" 2>&1 | tee -a "$EVID/versions.txt"
grep -a 'CuhubBridge .* loaded' "$GAME/BepInEx/LogOutput.log" | tail -1 | tee -a "$EVID/versions.txt"
grep -a 'krokmp\|KrokMP' "$GAME/BepInEx/LogOutput.log" | tail -3 | tee -a "$EVID/versions.txt"
command -v ss python3 >/dev/null && echo "ss/python3 ok" | tee -a "$EVID/versions.txt"
```

路径不对时先用 `ls "$HOME/.steam/steam/steamapps/common/"` 找到实际目录再改 `GAME`。

## 2. 取证命令（一次定义，通篇复用）

```bash
snap() {                     # snap R1-a  →  $EVID/R1-a/
  local dir="$EVID/$1"
  mkdir -p "$dir" || return 1
  date -Is > "$dir/time.txt"
  ss -lunp > "$dir/ss-lunp.txt" 2>&1
  ss -ltnp >> "$dir/ss-lunp.txt" 2>&1
  grep -aE 'Status|LiteNetLib|NetManager|CuhubBridge|Multiplayer' "$GAME/BepInEx/LogOutput.log" 2>/dev/null | tail -400 > "$dir/bepinex-tail.txt"
  local log
  log=$(ls -t "$CUHUB_DATA/Logs/"Log*.log 2>/dev/null | head -1)
  [ -n "$log" ] && cp "$log" "$dir/cuhub-log.txt"
  pgrep -af 'easytier-core' > "$dir/easytier.txt" 2>&1
  echo "snap -> $dir"
}
```

界面截图不强制用命令，但每个 `snap` 目录里应有当拍截图，命名固定 `ui-a.png`（A 侧）、`ui-b.png`（B 侧），便于跨用例对照。常用查询：

```bash
# 游戏进程与它实际监听的 UDP 端口（端口自动检测的唯一事实来源）
pgrep -af Casualties; ss -lunp | grep -E ':7790|:24444|:25565|:25003'
# 桥端口 41297 谁在用
ss -ltnp | grep 41297
# 隧道进程与参数（hostname / whitelist / port-forward 里烧死的端口）
pgrep -af easytier-core
# Cuhub 活动日志里与联机相关的行
grep -aE 'Multiplayer|Bridge' "$(ls -t "$CUHUB_DATA/Logs/"Log*.log | head -1)" | tail -40
# 游戏侧状态文案（§1.4 的原文都写在这里）
grep -a 'Status' "$GAME/BepInEx/LogOutput.log" | tail -8
```

## 3. 注入与恢复命令

五个注入件，**每条都带备份与恢复**；破坏性动作前先确认游戏/应用已退出（`pgrep -af Casualties`、`pgrep -af cuhub` 空）。

**I1 占住桥端口 41297（TCP）**

```bash
python3 -c 'import socket,time;s=socket.socket();s.bind(("127.0.0.1",41297));s.listen(1);print("holding tcp 41297",flush=True);time.sleep(900)'
ss -ltnp | grep 41297        # 另开终端确认
```

释放：在该终端 Ctrl-C，然后 `ss -ltnp | grep 41297` 应为空。

**I2 占住房间端口（UDP，示例 25003）**

```bash
python3 -c 'import socket,sys,time;p=int(sys.argv[1]);s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);s.bind(("0.0.0.0",p));print("holding udp",p,flush=True);time.sleep(900)' 25003
ss -lunp | grep 25003
```

不要给这个 socket 加 `SO_REUSEADDR`：桥的空闲探测（`mods/cuhub-bridge/CuhubBridge.Plugin/MpBackend.cs:183-201`）也是试绑 UDP，加复用会让「忙」被误判成「空闲」，注入失效。释放同上。

**I3 杀死桥宿主（Cuhub 进程）**

桥宿主住在 Cuhub 进程里，没有独立进程可杀；「杀桥」= 关掉 Cuhub。先记下启动方式，恢复时用同一条命令重开：

```bash
pgrep -af 'cuhub' | head
pkill -f 'cuhub' && sleep 1 && (pgrep -af cuhub || echo "cuhub stopped")
```

恢复：用桌面启动器或原来的启动命令重开 Cuhub。插件按 1/2/5/10/30 秒退避重连（`docs/bridge/protocol.md:21`），最长 30 秒内应重新出现会话。

**I4 移走 CUCoreLib.dll**

```bash
cd "$GAME/BepInEx/plugins" || exit 1
dll=$(find . -iname 'cucorelib.dll' -print -quit); echo "target=$dll"
[ -n "$dll" ] && cp -a "$dll" "$dll.w4bak" && mv "$dll" "$dll.disabled"
ls -l "$dll.w4bak" "$dll.disabled"
```

恢复：

```bash
cd "$GAME/BepInEx/plugins" || exit 1
d=$(find . -iname 'cucorelib.dll.disabled' -print -quit); echo "restore=$d"
[ -n "$d" ] && back="$(dirname "$d")/$(basename "$d" .disabled)" && mv "$d" "$back" && rm -f "$back.w4bak"
ls -l "$(find . -iname 'cucorelib.dll' -print -quit)"
```

BepInEx 只加载 `.dll`，改名即停用；备份留成 `.w4bak` 不会被加载。

**I5 改桥控制权限（默认关）**

应用必须已退出，否则界面会把文件覆盖回去。备份后写入「已允许」：

```bash
cd "$CUHUB_DATA" || exit 1
cp -a Settings.json Settings.json.w4bak
python3 - <<'PY'
import json, pathlib
p = pathlib.Path("Settings.json")
data = json.loads(p.read_text(encoding="utf-8"))
data.setdefault("bridge", {}).setdefault("permissions", {})["dev.cuhub.bridge"] = {
    "mp.host": "granted", "mp.join": "granted"}
p.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
grep -A4 'dev.cuhub.bridge' Settings.json
```

回到「默认关」（删掉授权条目）：

```bash
cd "$CUHUB_DATA" || exit 1
python3 - <<'PY'
import json, pathlib
p = pathlib.Path("Settings.json")
data = json.loads(p.read_text(encoding="utf-8"))
data.get("bridge", {}).get("permissions", {}).pop("dev.cuhub.bridge", None)
p.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
```

整体恢复：`cd "$CUHUB_DATA" && cp -a Settings.json.w4bak Settings.json && rm -f Settings.json.w4bak`。字段含义见 `crates/core/src/settings.rs:20-42` 与 `crates/core/src/bridge/command.rs:106-110`；「允许」= `mp.host` 与 `mp.join` 都为 `granted`（`crates/core/src/hub/bridge_host.rs:330-340`）。

**游戏内菜单操作（无 shell 注入）**

游戏内改地址/建房都走 UI，命令只能取证，不能注入：

1. 主菜单 →「打开多人联机模组菜单」。
2. 取消勾选「使用 Steam」（走 LiteNetLib 直连，端口才可指定）。
3.「通过 IP 直接连接」填 `127.0.0.1:<端口>`；名称栏即玩家名称。
4. 「启动主机」= 游戏内开房；「加入游戏」= 游戏内加房。
5. 停房：同一面板的停止按钮，或退到主菜单。

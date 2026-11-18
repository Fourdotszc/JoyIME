# 系统输入法（TSF）和后台程序之间的协议

JoyIME 在 Windows 上分两块：

- **joyime-tsf.dll**：TSF 文本服务（输入法）。用户在系统输入法列表（Win+空格）里选了 JoyIME，Windows 就把它加载进
  当前在打字的那个程序。它负责：在文字里显示正在打的拼音（组字串）、把选好的字上屏、告诉后台光标在哪里。
- **joyime-app.exe**（后台程序）：读摇杆、跑输入状态机、画候选窗和设置界面。**只有当某个程序里 JoyIME 是当前输入法、
  并且那个程序有焦点时**，摇杆才会打字。

两者之间用命名管道通信。

## 管道

- 名字：`\\.\pipe\joyime-ime`
- 后台程序是服务端，可以同时连很多个客户端（每个加载了 DLL 的进程里的每个 UI 线程一个连接）。
- 管道的安全描述符允许 AppContainer（商店应用、新版记事本）和低完整性进程连接：
  `D:(A;;GA;;;WD)(A;;GA;;;AC)S:(ML;;NW;;;LW)`
- DLL 连不上（后台没在跑）时，启动后台程序 `joyime-app.exe --service`，再重试连接。
  后台程序路径：DLL 所在目录下的 `joyime-app.exe`；注册表 `HKCU\Software\JoyIME` 的 `ServicePath`（字符串）可以覆盖。
  启动和重连都在后台线程里做，**绝不能卡住宿主程序的界面线程**。

## 消息

UTF-8 JSON，一行一条（以 `\n` 结尾），字段 `t` 是类型。

### DLL → 后台

| 消息 | 什么时候发 |
|---|---|
| `{"t":"hello","v":1,"pid":1234,"exe":"notepad.exe"}` | 连上后第一条 |
| `{"t":"focus","on":true}` | 这个线程里 JoyIME 被激活、并且焦点在一个文本上下文里（`ITfThreadMgrEventSink::OnSetFocus` 拿到非空文档）。焦点离开、或者 JoyIME 被切走（Deactivate）时发 `"on":false` |
| `{"t":"caret","x":10,"y":20,"w":1,"h":18}` | 光标（插入点，组字时是组字串末尾）在屏幕上的矩形，**物理像素**。焦点进来时、每次编辑后、选区变化时发；没变就不发 |

### 后台 → DLL

| 消息 | DLL 要做的 |
|---|---|
| `{"t":"comp","text":"ni'hao"}` | 设置组字串：没有组字就开始一个（`ITfContextComposition::StartComposition`），有就整段替换成 text。text 为空 = 删掉组字串并结束组字（不上屏） |
| `{"t":"commit","text":"你好"}` | 上屏：有组字串就用 text 替换它并结束组字；没有就在当前选区插入 text |

退格、回车这类按键不走管道，后台程序直接模拟按键（当前程序一定有焦点）。

## 后台程序怎么用这些消息

- 记下最后一个发 `focus on:true` 的连接作为「当前客户端」。它发 `on:false` 或断开，就没有当前客户端。
- 有当前客户端时：上屏走它的 `commit`；拼写区变化发 `comp`；候选窗贴着它报的 `caret` 放；刚收到 `focus on:true` 时弹方向提示。
- 没有当前客户端时：摇杆不打字（JoyIME 不是当前输入法）。
- 非 Windows 平台还没有原生输入法，继续用模拟键入（设置里可以选）。

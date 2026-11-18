# joyime-tsf：JoyIME 的 Windows 系统输入法

TSF 文本服务 DLL。用户在系统输入法列表（Win+空格）里选了 JoyIME，Windows 就把它加载进正在打字的程序。
它不读摇杆，只负责组字串、上屏和报告光标位置，其余交给后台程序 `joyime-app.exe`。协议见
[docs/ime-protocol.md](../../docs/ime-protocol.md)。

| 文件 | 内容 |
|---|---|
| `src/lib.rs` | GUID、DLL 入口、COM 类工厂、`Live`（对象计数，决定能不能卸载 DLL） |
| `src/register.rs` | `DllRegisterServer` / `DllUnregisterServer`：COM、TSF 语言配置和类别、后台程序路径 |
| `src/service.rs` | 文本服务：激活 / 停用、焦点、编辑会话（组字、上屏）、光标位置、组字串显示属性 |
| `src/pipe.rs` | 后台线程：连管道、按需启动后台程序、收发消息 |
| `src/log.rs` | 调试日志（`JOYIME_TSF_LOG` 或注册表 `HKCU\Software\JoyIME\TsfLog`） |
| `examples/tsf_host.rs` | 测试宿主：只在自己进程里切到 JoyIME，用来端到端测试 |

## 编译

```bash
cargo build -p joyime-tsf --release
cargo build -p joyime-tsf --release --target i686-pc-windows-msvc
```

64 位程序加载 64 位 DLL，32 位程序加载 32 位 DLL，两个都要。

## 开发时注册 / 反注册

```bash
powershell -ExecutionPolicy Bypass -File tools/dev-install-ime.ps1
powershell -ExecutionPolicy Bypass -File tools/dev-install-ime.ps1 -Uninstall
```

会弹 UAC。注册后在 设置 → 时间和语言 → 语言和区域 → 中文 → 键盘 里能看到「JoyIME 摇杆输入法」。
改了 DLL 要重新跑一遍；已经加载了旧 DLL 的程序要重启才会用新的。

## 端到端测试（不用真摇杆）

```bash
cargo build -p joyime-app
cargo build -p joyime-tsf --example tsf_host --release
JOYIME_SCRIPT=app/tests/e2e-notepad.txt JOYIME_EXIT=1 ./target/debug/joyime-app.exe --hidden --replace &
./target/release/examples/tsf_host.exe out.txt 16
cat out.txt    # TEXT=你好枕电脑我喜欢你。 ACTIVATED=true
```

`tsf_host` 会把输入法切到 JoyIME。Windows 默认所有窗口共用一个输入法，所以测试期间别的窗口也会是 JoyIME，
`tsf_host` 退出前会切回原来的输入法。

## 注意

- DLL 会进到用户的每个程序里：所有入口都包了 panic，界面线程上不做阻塞的事。
- 商店应用（AppContainer）要能读 DLL 所在目录，安装脚本里用 icacls 给了 `ALL APPLICATION PACKAGES` 读权限；
  管道的安全描述符也允许它们连（后台程序那边设置）。商店应用里不一定能启动后台程序，所以后台程序最好已经在跑。

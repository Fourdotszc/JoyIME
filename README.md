# JoyIME 摇杆输入法

用**街机摇杆**或**游戏手柄**打字的输入法。摇杆的八个方向加中位，正好对应手机九宫格：推一下方向就是按一下数字键，
上排按键选候选。支持中文九宫格（带拼音栏）、简拼、全拼、英文九宫格联想、逐个字母和数字符号。

在 Windows 上它是一个真正的系统输入法：在输入法列表（Win + 空格）里选「JoyIME 摇杆输入法」，
点进任何输入框就能用，候选窗贴在光标旁边。

## 安装（Windows 10 / 11）

1. 到 [Releases](../../releases) 下载 `JoyIME_x.y.z_x64-setup.exe`，运行（需要管理员权限，用来注册系统输入法）。
2. 插上摇杆，按 **Win + 空格** 切到「JoyIME 摇杆输入法」。
3. 点进一个输入框，推杆打字。设置窗口（按键映射、输入方式、练习区）在开始菜单里，或者单击托盘图标。

卸载：设置 → 应用 → JoyIME → 卸载。

## 怎么打字

| 操作 | 默认按键（8 键街机摇杆） |
|---|---|
| 推一下方向 = 九宫格一个数字键 | 摇杆（↖ = 5 键） |
| 选第 1–4 个候选 | 上排 4 个键 |
| 空格 / 选第一个候选 | 下排第 1 个 |
| 退格 | 下排第 2 个 |
| 下一页 / 切换输入方式 | 下排第 3 个 |
| 上一页 / 回车 | 下排第 4 个 |
| **选拼音**（9426 是 xian 还是 xiao） | **按住空格 + 上排键** |

- 中文九宫格：候选上面一行是拼音，编号对应上排键。按住空格再按上排键就定下那个拼音，候选只剩这个读音的字；
  多音节可以按住空格一个个选。点一下空格还是选第一个候选。
- 常用词会被记住，下次排在前面（设置里可以关、可以清空）。
- 6 键摇杆和游戏手柄有各自的预设；任何按键都可以在设置里改，不认识的摇杆可以用「检测按键」。

## 从源码编译

需要 Rust（加上 `i686-pc-windows-msvc` 目标，用来编 32 位输入法 DLL）和 Node.js。

```bash
cargo test -p joyime-pinyin -p joyime-english -p joyime-core   # 单元测试
cd app && npm ci && npx tauri build                            # 安装包：target/release/bundle/nsis/
```

开发时的调试、端到端测试见 [docs/testing.md](docs/testing.md)，输入法 DLL 见 [crates/joyime-tsf](crates/joyime-tsf/README.md)。

| 目录 | 内容 |
|---|---|
| `crates/joyime-pinyin` | 拼音引擎：九宫格数字 / 全拼 / 简拼 → 候选 |
| `crates/joyime-english` | 英文九宫格联想 |
| `crates/joyime-core` | 输入状态机：摇杆方向和按键 → 拼写、候选、上屏 |
| `crates/joyime-tsf` | Windows 系统输入法（TSF 文本服务 DLL） |
| `app` | 后台程序（Tauri）：读摇杆、候选悬浮窗、设置窗口 |
| `data` / `vendor` | 词库和它们的来源 |
| `prototype` | 最早的网页原型 |

## 词库来源

- 中文：Android 开源项目的谷歌拼音词库，Apache License 2.0（见 [vendor/googlepinyin/NOTICE.md](vendor/googlepinyin/NOTICE.md)）
- 英文：[FrequencyWords](https://github.com/hermitdave/FrequencyWords)，MIT（见 `vendor/english/`）

## 路线

- [x] Windows 系统输入法、安装包
- [ ] macOS（InputMethodKit）
- [ ] Linux（fcitx5 / IBus）

## 协议

[MIT](LICENSE)。词库各自遵循上面列出的协议。

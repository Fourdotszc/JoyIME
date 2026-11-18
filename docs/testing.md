# 测试

## 单元测试

```bash
cargo test -p joyime-pinyin -p joyime-english -p joyime-core
```

- `joyime-pinyin`：和网页原型逐条对照（`tests/golden.json` 由 `node tools/make_pinyin_golden.js` 生成）
- `joyime-core`：状态机，按 16ms 一帧模拟摇杆操作

## 端到端（Windows）：真的把字打进别的程序

不用真摇杆：程序按脚本模拟操作，一个带文本框的小窗口接收打出来的字。

```bash
cargo build -p joyime-app
JOYIME_SCRIPT=app/tests/e2e-notepad.txt JOYIME_LOG=e2e.log JOYIME_EXIT=1 ./target/debug/joyime-app.exe --hidden --replace &
powershell -NoProfile -ExecutionPolicy Bypass -File app/tests/e2e-textbox.ps1 e2e-out.txt 16
cat e2e-out.txt   # 应为 TEXT=你好枕电脑我喜欢你。 LOST_FOCUS=0
```

- 另一个脚本 `app/tests/e2e-digits.txt`：切到 123 层打数字，应为 `TEXT=2026.19`
- `JOYIME_SCRIPT`：回放脚本，格式见 `app/src-tauri/src/script.rs`
- `JOYIME_LOG`：记下每个上屏动作、拼写区变化、超过 30ms 的卡顿
- `JOYIME_EXIT`：脚本放完自动退出
- `--hidden`：不弹设置窗口（它会抢焦点）
- `--replace`：已经有 JoyIME 在跑（比如用户程序里的输入法 DLL 自动拉起来的）就让它退出、自己接手
- `JOYIME_CONFIG_DIR`：设置、按键映射、用户词库放到这个目录，测试时不碰真正的配置
- 另一个脚本 `app/tests/e2e-learn.txt`：常用词，应为 `TEXT=先先`，词库里记下 `cn9	9426	先	2`
- 另一个脚本 `app/tests/e2e-pinyin.txt`：九宫格拼音栏（选拼音键定下 xian、ni），应为 `TEXT=先你好`
- 另一个脚本 `app/tests/e2e-pinyin-chord.txt`：按住空格 + 上排键选拼音，应为 `TEXT=哟你好`
- `LOST_FOCUS` 不为 0 说明有窗口抢了焦点（比如候选悬浮窗），字会打丢
- 日志里的「悬浮窗 Some((Hint, Some(Rect…)))」：悬浮窗当前的样子和它贴着的光标位置

## 端到端（Windows）：走系统输入法（TSF）

先注册 DLL（`tools/dev-install-ime.ps1`，要管理员），然后用测试宿主代替文本框：

```bash
cargo build -p joyime-tsf --example tsf_host --release
JOYIME_SCRIPT=app/tests/e2e-notepad.txt JOYIME_LOG=e2e.log JOYIME_EXIT=1 ./target/debug/joyime-app.exe --hidden --replace &
./target/release/examples/tsf_host.exe tsf-out.txt 15
cat tsf-out.txt   # TEXT=你好枕电脑我喜欢你。 ACTIVATED=true
```

日志里上屏应该是 `Commit(..) -> 系统输入法`；DLL 自己的日志在 `HKCU\Software\JoyIME\TsfLog` 指的文件里。

## 看悬浮窗贴在光标旁边的样子

`app/tests/overlay-shot.txt` 打一半停住，配合截屏脚本（物理像素坐标）：

```bash
JOYIME_SCRIPT=app/tests/overlay-shot.txt JOYIME_EXIT=1 ./target/debug/joyime-app.exe --hidden --replace &
powershell -File app/tests/e2e-textbox.ps1 box.txt 14 &
powershell -File app/tests/screen-grab.ps1 shot.png 1150 700 1400 600 6
```

## 光标 / 焦点探测

```bash
cargo run -p joyime-app --example caret_probe
```

每 500ms 打印一次当前焦点能不能打字、光标位置、用哪种办法查到的（gui-caret / msaa / uia）。
在记事本、浏览器、各种聊天软件之间切换，看光标位置对不对。

- `JOYIME_OVERLAY_MONITOR=0,1`：悬浮窗每次弹出轮流放到第 0、1 块屏（测多屏不同缩放）

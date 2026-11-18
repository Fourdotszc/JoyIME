"""README 用的截图：用无界面 Chrome 打开设置窗口 / 候选窗的页面（浏览器里没有后台程序，用页面自带的演示数据），
存到 docs/images/。中文界面存 xxx.png（中文 README 用），英文界面存 xxx-en.png（英文 README 用）。

    python tools/screenshots.py
"""
import os
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
UI = (ROOT / "app" / "ui").as_uri()
OUT = ROOT / "docs" / "images"
CHROME = os.environ.get("CHROME", r"C:\Program Files\Google\Chrome\Application\chrome.exe")

SHOTS = [
    # 文件名, 页面, 窗口大小, 等多久（毫秒，演示数据 600ms 推一次）
    ("settings-controller", "settings.html?#controller", "1180,760", 2500),
    ("settings-input", "settings.html?#input", "1180,760", 2500),
    ("settings-practice", "settings.html?#practice", "1180,760", 2500),
    ("overlay", "overlay.html?", "900,150", 1500),
    ("overlay-pyfocus", "overlay.html?demo=pyfocus&", "900,150", 1500),
    ("overlay-pinned", "overlay.html?demo=pinyin&", "900,150", 1500),
]
LANGS = [("zh", ""), ("en", "-en")]


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as profile:
        for (name, page, size, wait), (lang, suffix) in ((s, l) for s in SHOTS for l in LANGS):
            path = OUT / f"{name}{suffix}.png"
            # 页面地址里的 ? 后面插上语言（# 要留在最后）
            page = page.replace("?", f"?lang={lang}&", 1)
            subprocess.run(
                [
                    CHROME,
                    "--headless",
                    "--disable-gpu",
                    "--hide-scrollbars",
                    f"--user-data-dir={profile}",
                    "--force-device-scale-factor=1.5",
                    "--default-background-color=00000000",
                    f"--window-size={size}",
                    f"--virtual-time-budget={wait}",
                    f"--screenshot={path}",
                    f"{UI}/{page}",
                ],
                check=True,
                capture_output=True,
            )
            if name.startswith("overlay"):
                trim(path)
            print(path, path.stat().st_size)


def trim(path):
    """候选窗页面是透明的，面板只占左上角一块：按不透明的部分裁掉多余的空白（留一点阴影）"""
    try:
        from PIL import Image
    except ImportError:
        return
    im = Image.open(path).convert("RGBA")
    box = im.getchannel("A").point(lambda a: 255 if a > 8 else 0).getbbox()
    if box:
        pad = 6
        l, t, r, b = box
        im.crop((max(l - pad, 0), max(t - pad, 0), min(r + pad, im.width), min(b + pad, im.height))).save(path)


if __name__ == "__main__":
    main()

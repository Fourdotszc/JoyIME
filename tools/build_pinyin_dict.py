"""把谷歌拼音的 rawdict 转成输入法用的拼音词库。

rawdict 每行：词 词频 是否生僻(0/1) 音节1 音节2 ...（UTF-16）
输出每行：音节用 ' 连起来<TAB>词 词频<TAB>词 词频 ...，同一拼音下按词频从高到低。
同一份内容写两个文件：
  data/pinyin.txt            纯文本，Rust 核心库编译时直接嵌进去
  prototype/pinyin-dict.js   包成 JS 变量，给网页原型用

用法：python -I tools/build_pinyin_dict.py vendor/googlepinyin/rawdict_utf16_65105_freq.txt data/pinyin.txt prototype/pinyin-dict.js
"""
import json
import sys
from collections import defaultdict


def main(src, dst_txt, dst_js):
    groups = defaultdict(list)
    with open(src, encoding="utf-16") as f:
        for line in f:
            parts = line.split()
            if len(parts) < 4:
                continue
            word, freq, syllables = parts[0], float(parts[1]), parts[3:]
            if len(syllables) != len(word):
                continue
            groups["'".join(syllables)].append((word, max(1, round(freq))))

    lines = []
    for py in sorted(groups):
        items = sorted(groups[py], key=lambda x: -x[1])
        lines.append(py + "\t" + "\t".join(f"{w} {n}" for w, n in items))
    body = "\n".join(lines)

    with open(dst_txt, "w", encoding="utf-8", newline="\n") as f:
        f.write(body + "\n")
    with open(dst_js, "w", encoding="utf-8", newline="\n") as f:
        f.write("// 由 tools/build_pinyin_dict.py 从谷歌拼音词库（AOSP PinyinIME，Apache-2.0）生成，不要手改\n")
        f.write("var PINYIN_DICT = ")
        f.write(json.dumps(body, ensure_ascii=False))
        f.write(";\n")
    print(f"{len(lines)} 个拼音，{sum(len(v) for v in groups.values())} 个词 -> {dst_txt}, {dst_js}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])

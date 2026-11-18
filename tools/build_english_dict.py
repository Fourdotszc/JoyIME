"""构建英文九宫格词表。

原始词表将部分缩写拆为词干和撇号后的碎片，无法直接用于 T9。
常用缩写的频次按其词干在原表的频次乘固定比例估计；比例是保守的
常见用法占比，且脚本会在输出中只保留估计后的带撇号形式。

用法：python -I tools/build_english_dict.py INPUT OUTPUT
"""
import sys


# （带撇号形式，拆分后的词干，估计其占词干频次的比例）。
# 例如 don 在该语料中几乎全是 don't 的拆分词干，所以取 75%；can
# 同时是独立词，can't 只取 25%。这些是可审查、可重复的近似而非声称的
# 原始双词共现计数（原始文件未提供该信息）。
CONTRACTIONS = (
    ("don't", "don", 0.75), ("can't", "can", 0.25), ("won't", "won", 0.80),
    ("isn't", "is", 0.12), ("aren't", "are", 0.12), ("weren't", "were", 0.12),
    ("wasn't", "was", 0.12), ("didn't", "did", 0.18), ("doesn't", "does", 0.18),
    ("couldn't", "could", 0.18), ("wouldn't", "would", 0.18),
    ("shouldn't", "should", 0.18), ("haven't", "have", 0.12), ("hasn't", "has", 0.12),
    ("i'm", "i", 0.12), ("i'll", "i", 0.08), ("i've", "i", 0.04), ("i'd", "i", 0.04),
    ("it's", "it", 0.20), ("you're", "you", 0.10), ("you'll", "you", 0.05),
    ("you've", "you", 0.03), ("you'd", "you", 0.03), ("we're", "we", 0.12),
    ("we'll", "we", 0.05), ("we've", "we", 0.03), ("we'd", "we", 0.03),
    ("they're", "they", 0.12), ("they'll", "they", 0.05),
    ("they've", "they", 0.03), ("they'd", "they", 0.03), ("that's", "that", 0.15),
    ("there's", "there", 0.15), ("what's", "what", 0.15), ("let's", "let", 0.20),
    ("he's", "he", 0.08), ("she's", "she", 0.08), ("who's", "who", 0.08),
    ("where's", "where", 0.08), ("how's", "how", 0.08),
)

# 拆分残片，以及只是不带撇号的缩写拼写。保留本身独立且常用的词，
# 如 its、ill、were，避免把正常英文误删。
FRAGMENTS = {"don", "s", "t", "m", "re", "ll", "ve"}
UNAPOSTROPHIZED = {
    "dont", "cant", "wont", "isnt", "arent", "wasnt", "werent", "didnt",
    "doesnt", "couldnt", "wouldnt", "shouldnt", "havent", "hasnt", "im",
    "youre", "theyre", "thats", "theres", "whats", "lets", "youll", "youve",
    "ive", "id", "wed", "theyd",
}


def is_lower_word(word):
    return word.isascii() and word.isalpha() and word.islower() and (len(word) > 1 or word in {"a", "i"})


def main(src, dst):
    raw = {}
    with open(src, encoding="utf-8") as f:
        for line in f:
            parts = line.split()
            if len(parts) != 2:
                continue
            word, count_text = parts
            if not is_lower_word(word):
                continue
            try:
                raw[word] = max(raw.get(word, 0), int(count_text))
            except ValueError:
                continue

    words = {
        word: count
        for word, count in raw.items()
        if word not in FRAGMENTS and word not in UNAPOSTROPHIZED
    }
    for word, stem, proportion in CONTRACTIONS:
        stem_count = raw.get(stem)
        if stem_count is not None:
            words[word] = max(words.get(word, 0), round(stem_count * proportion))

    ordered = sorted(words.items(), key=lambda item: (-item[1], item[0]))
    with open(dst, "w", encoding="utf-8", newline="\n") as f:
        for word, count in ordered:
            f.write(f"{word} {count}\n")
    print(f"保留 {len(ordered)} 个词 -> {dst}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("用法：python -I tools/build_english_dict.py INPUT OUTPUT")
    main(sys.argv[1], sys.argv[2])

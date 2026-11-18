'use strict';

// 拼音引擎：把连续输入切成词、列出候选，并用一元词频挑出整句。
// 不预先切音节，而是直接拿输入的子串去查「音节连起来的拼音」，所以 xian 能同时查到 先 和 西安。
// 用户打的 ' 是强制分隔：词只有在自己的音节边界上才能跨过它（xi'an 只给 西安）。
// 只有输入的最后一段可以是没打完的音节（nih → 你好）。
// 多音节词还能用每个音节的首字母来打（简拼：nh → 你好，zgr → 中国人），
// 结尾的简拼也可以没打完（wxh → 我喜欢、我喜欢你）。
//
// 两种输入：'letters' 是字母；'t9' 是九宫格数字（你好 = 64426），每个数字可能是键上任一字母，
// 做法完全一样，只是查的是「拼音换成数字」的那张表（简拼 nh = 64）。

// 用「没打完的拼音」匹配到的词，词频按 1/30 算：打 xian 时 先 仍排在 想(xiang) 前面，
// 但打 n 时 你(ni) 能排到 嗯(n) 前面
const PY_PREFIX_PENALTY = Math.log(30);
// 用简拼匹配到的词，词频按 1/10 算：两种读法都说得通时，完整拼音优先
const PY_ABBR_PENALTY = Math.log(10);
// 每个位置、每种长度，没打完的拼音和简拼最多各取几个词进候选；拼音完全对上的字词全部列出，
// 不然生僻字（甄）会被 正在、政府 这类词挤出去
const PY_PER_EDGE = 20;
const PY_MAX_CANDIDATES = 1000; // 九宫格两三个键（94 = yi/xi/zi…）完全对上的字就有好几百个

const T9_KEYS = ['abc', 'def', 'ghi', 'jkl', 'mno', 'pqrs', 'tuv', 'wxyz'];
const T9_DIGIT = {};
T9_KEYS.forEach((g, i) => { for (const c of g) T9_DIGIT[c] = String(i + 2); });
const toT9 = s => s.replace(/[a-z]/g, c => T9_DIGIT[c]);

function makeTable(index) {
  for (const arr of index.values()) arr.sort((a, b) => b.f - a.f);
  return { index, keys: [...index.keys()].sort(), cache: new Map() };
}

function push(index, key, entry) {
  let arr = index.get(key);
  if (!arr) index.set(key, (arr = []));
  arr.push(entry);
}

class PinyinEngine {
  constructor(dictText) {
    const index = new Map(); // 连起来的拼音 -> [{ w, f, py, cuts }]
    const abbr = new Map();  // 多音节词的首字母 -> 同样的结构，py 是首字母，每个字母都是一个音节
    this.sylFreq = new Map(); // 音节 -> 单字词频之和，给拼音选择栏排序
    let total = 0;
    for (const line of dictText.split('\n')) {
      if (!line) continue;
      const [py, ...items] = line.split('\t');
      const syl = py.split("'");
      const key = syl.join('');
      const cuts = []; // 词内部的音节边界（相对词首的字母数）
      let p = 0;
      for (let i = 0; i < syl.length - 1; i++) cuts.push(p += syl[i].length);
      const initials = syl.map(x => x[0]).join('');
      const abbrCuts = syl.slice(1).map((_, i) => i + 1);
      for (const it of items) {
        const sp = it.lastIndexOf(' ');
        const w = it.slice(0, sp);
        const f = +it.slice(sp + 1);
        push(index, key, { w, f, py: key, cuts });
        // 单字也收进简拼表（n → 你），这样整句里可以混着用：nhao → 你好，wxhn → 我喜欢你
        push(abbr, initials, { w, f, py: initials, cuts: abbrCuts });
        total += f;
        if (syl.length === 1) this.sylFreq.set(key, (this.sylFreq.get(key) || 0) + f);
      }
    }
    this.tables = { letters: { full: makeTable(index), abbr: makeTable(abbr) } };
    this.logTotal = Math.log(total);
  }

  // 九宫格表第一次用到时才建：把字母表的 key 换成数字
  table(mode) {
    if (mode === 't9' && !this.tables.t9) {
      const toDigits = t => {
        const index = new Map();
        for (const [key, arr] of t.index) for (const e of arr) push(index, toT9(key), e);
        return makeTable(index);
      };
      const { full, abbr } = this.tables.letters;
      this.tables.t9 = { full: toDigits(full), abbr: toDigits(abbr) };
    }
    return this.tables[mode];
  }

  cost(f) {
    return this.logTotal - Math.log(f);
  }

  // 九宫格数字串开头可能是哪些完整音节：长的在前，同样长按常用程度
  // （9426 → xiao xian zhao zhan xia zha xi yi zi）
  syllableOptions(digits) {
    const out = [];
    for (const [s, f] of this.sylFreq) if (digits.startsWith(toT9(s))) out.push({ s, f });
    return out.sort((a, b) => b.s.length - a.s.length || b.f - a.f).map(x => x.s);
  }

  // 所有 key 以 prefix 开头、但不等于 prefix 的词，按词频降序
  prefixed(t, prefix) {
    let hit = t.cache.get(prefix);
    if (hit) return hit;
    let lo = 0, hi = t.keys.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (t.keys[mid] < prefix) lo = mid + 1; else hi = mid;
    }
    hit = [];
    for (let k = lo; k < t.keys.length && t.keys[k].startsWith(prefix); k++) {
      if (t.keys[k] !== prefix) hit.push(...t.index.get(t.keys[k]));
    }
    hit.sort((a, b) => b.f - a.f);
    t.cache.set(prefix, hit);
    return hit;
  }

  // raw：用户打的字母（或九宫格数字）和 '。
  // 返回候选 [{ w, rawLen, reading }]：rawLen 是选它之后要从 raw 开头删掉多少个字符，
  // reading 是它对应的那段输入读作什么拼音（九宫格模式下用来告诉用户「这串键被理解成了什么」）。
  // letters：九宫格模式下用户已经在拼音栏里定下来的开头字母（各音节之间要在 raw 里用 ' 隔开），
  // 只保留读音和它一致、并且不会在定下的音节中间断开的词。
  query(raw, mode = 'letters', letters = '') {
    const { full, abbr } = this.table(mode);
    const bounds = new Set();
    let s = '';
    const rawPos = [0]; // s 里第 i 个字符之前，对应 raw 里的位置
    for (let r = 0; r < raw.length; r++) {
      if (raw[r] === "'") {
        if (s) bounds.add(s.length);
      } else {
        s += raw[r];
      }
      rawPos[s.length] = r + 1;
    }
    const n = s.length;
    if (!n) return [];

    // 词 e 放在 s[i..j) 上时，用户的强制分隔必须落在它的音节边界上
    const L = letters.length;
    const fits = (e, i, j) => {
      for (let b = i + 1; b < j; b++) if (bounds.has(b) && !e.cuts.includes(b - i)) return false;
      // 结尾打了 '（zhen'）表示这个音节已经完整，没打完的词必须在这里正好换下一个音节（不给 正 zheng）
      if (bounds.has(j) && i + e.py.length > j && !e.cuts.includes(j - i)) return false;
      if (!L) return true;
      if (j < L && !bounds.has(j)) return false;
      for (let p = i; p < Math.min(j, L); p++) if (e.py[p - i] !== letters[p]) return false;
      // 定下的音节不能被拆开（xian 不给 西安），最后一个也不能被接着拼长（xian 不给 想 xiang）
      for (const c of e.cuts) if (i + c < L && !bounds.has(i + c)) return false;
      if (L > i && L < i + e.py.length && !e.cuts.includes(L - i)) return false;
      return true;
    };
    const take = (list, i, j, penalty, limit) => {
      const out = [];
      for (const e of list || []) {
        if (out.length >= limit) break;
        if (fits(e, i, j)) out.push({ j, e, cost: this.cost(e.f) + penalty });
      }
      return out;
    };
    const byCost = (a, b) => a.cost - b.cost;
    // 从 i 出发的所有边，吃掉输入多的排前面，同样长的按代价排。只有吃到结尾的才可以是没打完的
    // （完整拼音没打完，或者简拼没打完；简拼至少要两个字母，不然打一个 n 就会冒出所有 n 开头的词）。
    // exactLimit：拼音完全对上的取几个。列候选时不限，算整句时每组只要最好的一个。
    const edgesFrom = (i, limit, exactLimit = limit) => {
      const rem = s.slice(i);
      const fuzzy = [
        ...take(this.prefixed(full, rem), i, n, PY_PREFIX_PENALTY, limit),
        ...take(abbr.index.get(rem), i, n, PY_ABBR_PENALTY, limit),
        ...(rem.length >= 2 ? take(this.prefixed(abbr, rem), i, n, PY_ABBR_PENALTY + PY_PREFIX_PENALTY, limit) : []),
      ].sort(byCost).slice(0, limit);
      const out = [...take(full.index.get(rem), i, n, 0, exactLimit), ...fuzzy].sort(byCost);
      for (let j = n - 1; j > i; j--) {
        const key = s.slice(i, j);
        out.push(...[
          ...take(full.index.get(key), i, j, 0, exactLimit),
          ...take(abbr.index.get(key), i, j, PY_ABBR_PENALTY, limit),
        ].sort(byCost));
      }
      return out;
    };
    // 词 e 的拼音里，对应输入那 len 个字符的部分，音节之间加 '
    const reading = (e, len) => {
      let out = '';
      for (let k = 0; k < len; k++) out += (e.cuts.includes(k) ? "'" : '') + e.py[k];
      return out;
    };

    // 整句：一元模型的最短路（每个词的代价 = -log 词频占比）
    const best = new Array(n + 1).fill(Infinity);
    const back = new Array(n + 1).fill(null);
    best[0] = 0;
    for (let i = 0; i < n; i++) {
      if (best[i] === Infinity) continue;
      for (const edge of edgesFrom(i, 1)) {
        const c = best[i] + edge.cost;
        if (c < best[edge.j]) { best[edge.j] = c; back[edge.j] = { i, e: edge.e }; }
      }
    }

    const cands = [];
    const seen = new Set();
    const add = (w, len, rd) => {
      if (seen.has(w) || cands.length >= PY_MAX_CANDIDATES) return;
      seen.add(w);
      // 选完后顺手把紧跟着的 ' 也删掉
      let rawLen = rawPos[len];
      while (raw[rawLen] === "'") rawLen++;
      cands.push({ w, rawLen, reading: rd });
    };

    if (best[n] !== Infinity) {
      const parts = [];
      for (let p = n; p > 0; p = back[p].i) parts.unshift({ e: back[p].e, len: p - back[p].i });
      if (parts.length > 1) {
        add(parts.map(x => x.e.w).join(''), n, parts.map(x => reading(x.e, x.len)).join("'"));
      }
    }
    for (const edge of edgesFrom(0, PY_PER_EDGE, Infinity)) add(edge.e.w, edge.j, reading(edge.e, edge.j));
    return cands;
  }
}

if (typeof module !== 'undefined') module.exports = { PinyinEngine, toT9 };

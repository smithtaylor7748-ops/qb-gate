/**
 * 酒馆控制台里的 ANSI 颜色码（2026-09-29）。
 *
 * SillyTavern 的 `color.red(...)` 不管输出到终端还是文件都带着颜色码（1.19.0 `src/util.js`），
 * 面板把它的输出收进日志文件之后，原样显示就是一行行 `\u001b[31m…\u001b[39m`。这里只认
 * SGR（`ESC [ … m`）里的前景色与粗体；别的控制序列（清行、移光标、OSC 标题）一律去掉，不执行。
 *
 * ⛔ 颜色不在这里写死：每种颜色映射到一个 CSS 类（`ansi-red` …），类在 `tavern.css` 里引用 `tokens.css` 的变量。
 */

export type AnsiTone =
  "default" | "red" | "green" | "yellow" | "blue" | "magenta" | "cyan" | "gray";

export interface AnsiSegment {
  text: string;
  tone: AnsiTone;
  bold: boolean;
}

const FG: Record<number, AnsiTone> = {
  30: "gray",
  31: "red",
  32: "green",
  33: "yellow",
  34: "blue",
  35: "magenta",
  36: "cyan",
  37: "default",
  90: "gray",
  91: "red",
  92: "green",
  93: "yellow",
  94: "blue",
  95: "magenta",
  96: "cyan",
  97: "default",
};

/** 一行 → 若干段（相邻同样式的合并）。空行给一段空文本，界面照样占一行。 */
export function parseAnsi(line: string): AnsiSegment[] {
  const out: AnsiSegment[] = [];
  let tone: AnsiTone = "default";
  let bold = false;
  let text = "";
  const flush = () => {
    if (!text) return;
    const last = out[out.length - 1];
    if (last && last.tone === tone && last.bold === bold) last.text += text;
    else out.push({ text, tone, bold });
    text = "";
  };
  let i = 0;
  while (i < line.length) {
    const c = line[i];
    if (c !== "\u001b") {
      text += c;
      i++;
      continue;
    }
    const next = line[i + 1];
    if (next === "[") {
      // CSI：参数字节 0x30–0x3F、中间字节 0x20–0x2F，最后一个 0x40–0x7E 是命令。
      let j = i + 2;
      while (
        j < line.length &&
        line.charCodeAt(j) >= 0x20 &&
        line.charCodeAt(j) <= 0x3f
      )
        j++;
      const cmd = line[j];
      const params = line.slice(i + 2, j);
      i = j + 1;
      if (cmd !== "m") continue;
      flush();
      const codes =
        params === ""
          ? [0]
          : params.split(";").map((p) => parseInt(p, 10) || 0);
      for (let k = 0; k < codes.length; k++) {
        const n = codes[k];
        if (n === 0) {
          tone = "default";
          bold = false;
        } else if (n === 1) bold = true;
        else if (n === 22) bold = false;
        else if (n === 39) tone = "default";
        else if (n === 38 || n === 48) {
          // 256 色 / 真彩色：跳过它的参数，不认。
          k += codes[k + 1] === 5 ? 2 : codes[k + 1] === 2 ? 4 : 0;
          if (n === 38) tone = "default";
        } else if (FG[n]) tone = FG[n];
      }
      continue;
    }
    if (next === "]") {
      // OSC：到 BEL 或 ESC \ 为止，整段丢掉。
      let j = i + 2;
      while (
        j < line.length &&
        line[j] !== "\u0007" &&
        !(line[j] === "\u001b" && line[j + 1] === "\\")
      )
        j++;
      i = line[j] === "\u0007" ? j + 1 : j + 2;
      continue;
    }
    // 孤零零的 ESC 或别的两字节序列：去掉。
    i += next === undefined ? 1 : 2;
  }
  flush();
  return out.length ? out : [{ text: "", tone: "default", bold: false }];
}

/** 去掉全部控制序列的纯文本（复制、搜索用）。 */
export function stripAnsi(line: string): string {
  return parseAnsi(line)
    .map((s) => s.text)
    .join("");
}

import { describe, expect, it } from "vitest";
import { parseAnsi, stripAnsi } from "./ansi";

describe("parseAnsi", () => {
  it("reads SillyTavern's own red error line (color.red wraps the whole message)", () => {
    const line =
      "\u001b[31mAddress 127.0.0.1:8000 is already in use.\u001b[39m";
    expect(parseAnsi(line)).toEqual([
      {
        text: "Address 127.0.0.1:8000 is already in use.",
        tone: "red",
        bold: false,
      },
    ]);
  });

  it("keeps plain text around colored parts and merges equal neighbours", () => {
    const line =
      "SillyTavern is listening on\u001b[32m IPv4: 127.0.0.1:8000\u001b[39m";
    const segs = parseAnsi(line);
    expect(segs.map((s) => [s.text, s.tone])).toEqual([
      ["SillyTavern is listening on", "default"],
      [" IPv4: 127.0.0.1:8000", "green"],
    ]);
    expect(parseAnsi("a\u001b[31m\u001b[39mb")).toEqual([
      { text: "ab", tone: "default", bold: false },
    ]);
  });

  it("drops cursor movement, 256-colour parameters and OSC titles instead of printing them", () => {
    expect(stripAnsi("\u001b[2K\u001b[1Gnpm")).toBe("npm");
    expect(stripAnsi("\u001b]0;title\u0007after")).toBe("after");
    const s = parseAnsi("\u001b[38;5;208morange\u001b[0m plain");
    expect(s.map((x) => x.text).join("")).toBe("orange plain");
    expect(s[0].tone).toBe("default");
    expect(stripAnsi("lone \u001b")).toBe("lone ");
  });

  it("tracks bold and a full reset", () => {
    const s = parseAnsi("\u001b[1;33mwarn\u001b[0m ok");
    expect(s).toEqual([
      { text: "warn", tone: "yellow", bold: true },
      { text: " ok", tone: "default", bold: false },
    ]);
  });

  it("gives an empty line one empty segment so it still takes a row", () => {
    expect(parseAnsi("")).toEqual([{ text: "", tone: "default", bold: false }]);
  });
});

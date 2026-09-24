import { describe, expect, it } from "vitest";
import { parseNotes } from "./releaseNotes";

describe("parseNotes", () => {
  it("排出小标题、条目与段落", () => {
    expect(
      parseNotes("## 更新内容\n\n- 修了一个问题\n- 加了一个功能\n\n一句说明"),
    ).toEqual([
      { kind: "heading", text: "更新内容" },
      { kind: "item", text: "修了一个问题" },
      { kind: "item", text: "加了一个功能" },
      { kind: "text", text: "一句说明" },
    ]);
  });

  it("剥掉粗体、代码与链接标记，只留文字", () => {
    expect(
      parseNotes(
        "- **一键更新**：核对 `SHA256SUMS.txt`，见 [发布页](https://example.test)",
      ),
    ).toEqual([
      {
        kind: "item",
        text: "一键更新：核对 SHA256SUMS.txt，见 发布页",
      },
    ]);
  });

  it("跳过空行、HTML 注释与分隔线，兼容 CRLF", () => {
    expect(
      parseNotes("<!-- version: 0.25.3 -->\r\n---\r\n\r\n* 条目\r\n"),
    ).toEqual([{ kind: "item", text: "条目" }]);
  });

  it("空说明得到空列表", () => {
    expect(parseNotes("")).toEqual([]);
    expect(parseNotes("\n\n   \n")).toEqual([]);
  });
});

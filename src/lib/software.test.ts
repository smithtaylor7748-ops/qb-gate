import { describe, expect, it } from "vitest";

import type { Software } from "./generated/Software";
import {
  NOT_INSTALLED,
  NOT_YET_READ,
  VERSION_UNREADABLE,
  pillLabel,
  pillTone,
  versionLine,
} from "./software";

function sw(installed: boolean, version: string | null): Software {
  return {
    id: "x",
    name: "X",
    installed,
    version,
    path: null,
    advisory: null,
  };
}

describe("软件卡上「装没装、哪一版」的说法", () => {
  it("没装就是没装", () => {
    expect(versionLine(sw(false, null))).toBe(NOT_INSTALLED);
    expect(pillLabel(sw(false, null))).toBe(NOT_INSTALLED);
    expect(pillTone(sw(false, null))).toBe("default");
  });

  it("装了且读得出版本，两处显示同一个版本号", () => {
    expect(versionLine(sw(true, "2.1.268"))).toBe("2.1.268");
    expect(pillLabel(sw(true, "2.1.268"))).toBe("2.1.268");
    expect(pillTone(sw(true, "2.1.268"))).toBe("ok");
  });

  /**
   * ⛔ 这一条是整个文件存在的理由。
   *
   * 「装了但版本号读不出」跟「没装」是两件事（§7.20：`decide()` 把 `None`
   * 一律判成 FreshInstall，于是一个 218 MB 的 claude.exe 躺在盘上而界面写「未安装」，
   * 排查方向从 ACL 被带到了安装路径）。折成两档就会把这条教训丢掉。
   */
  it("装了但版本号读不出，既不是「未安装」也不是空白", () => {
    const s = sw(true, null);
    expect(versionLine(s)).toBe(VERSION_UNREADABLE);
    expect(versionLine(s)).not.toBe(NOT_INSTALLED);
    expect(versionLine(s)).not.toBe("");
    expect(pillLabel(s)).toBe("已装");
    expect(pillTone(s)).toBe("ok");
  });

  it("报告还没回来是第四档，不许折成「未安装」", () => {
    expect(versionLine(undefined)).toBe(NOT_YET_READ);
    expect(versionLine(undefined)).not.toBe(NOT_INSTALLED);
    expect(pillLabel(undefined)).toBe(NOT_YET_READ);
    expect(pillTone(undefined)).toBe("warn");
  });

  /**
   * Pill 与版本行**不可能互相矛盾** —— 这是 0.28.0 之前七张卡各写各的那个 bug
   * 的一般化：两处读同一份数据、走同一个函数族，才谈得上「对得上」。
   */
  it("Pill 与版本行在每一种组合下都指向同一个结论", () => {
    for (const installed of [true, false]) {
      for (const version of ["1.2.3", null]) {
        const s = sw(installed, version);
        const line = versionLine(s);
        const pill = pillLabel(s);
        const lineSaysInstalled = line !== NOT_INSTALLED;
        const pillSaysInstalled = pill !== NOT_INSTALLED;
        expect(lineSaysInstalled).toBe(pillSaysInstalled);
        expect(lineSaysInstalled).toBe(installed);
      }
    }
  });
});

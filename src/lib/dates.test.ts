import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { addDays, daysFrom, lastDays, shortDay, ymd } from "./dates";

describe("本地日历日期", () => {
  // ⛔ 这一组要在 UTC 以东跑：作者的机器在 UTC−4，原来那种 `toISOString()` 的写法
  // 在那里恰好对，在东八区整张趋势图错一天。Node 允许运行中改 TZ。
  const before = process.env.TZ;
  beforeAll(() => {
    process.env.TZ = "Asia/Shanghai";
  });
  afterAll(() => {
    process.env.TZ = before;
  });

  it("原来的写法在东八区会把本地零点算成前一天（回归用例的前提）", () => {
    const local = new Date("2026-09-24T00:00:00");
    expect(local.toISOString().slice(0, 10)).toBe("2026-09-23");
    expect(ymd(local)).toBe("2026-09-24");
  });

  it("一天一天往后数，不跳天、不丢最后一天", () => {
    expect(daysFrom("2026-09-18", "2026-09-24")).toEqual([
      "2026-09-18",
      "2026-09-19",
      "2026-09-20",
      "2026-09-21",
      "2026-09-22",
      "2026-09-23",
      "2026-09-24",
    ]);
  });

  it("跨月、跨年", () => {
    expect(addDays("2026-09-30", 1)).toBe("2026-10-01");
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
    expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
  });

  it("含今天的最近 N 天", () => {
    const w = lastDays(7, "2026-09-24");
    expect(w).toHaveLength(7);
    expect(w[0]).toBe("2026-09-18");
    expect(w[6]).toBe("2026-09-24");
    expect(lastDays(30, "2026-09-24")).toHaveLength(30);
  });

  it("坏日期不会拖成死循环", () => {
    expect(daysFrom("2026-09-24", "2026-09-01")).toEqual([]);
    expect(daysFrom("2000-01-01", "2099-01-01", 10)).toHaveLength(10);
  });

  it("短日期", () => {
    expect(shortDay("2026-09-24")).toBe("09-24");
  });
});

describe("夏令时那一天", () => {
  const before = process.env.TZ;
  beforeAll(() => {
    process.env.TZ = "America/New_York";
  });
  afterAll(() => {
    process.env.TZ = before;
  });

  // 2026-11-01 美东退出夏令时，那一天有 25 个小时 —— 按「加 86400 秒」数会重复一天。
  it("按日历加天，不按毫秒加", () => {
    expect(daysFrom("2026-10-31", "2026-11-02")).toEqual([
      "2026-10-31",
      "2026-11-01",
      "2026-11-02",
    ]);
  });
});

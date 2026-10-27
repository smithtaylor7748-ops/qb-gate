// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import YearHeatmap, { heatGrid, heatLevels } from "./YearHeatmap";

afterEach(cleanup);

describe("全年热力图", () => {
  it("splits busy days by quartile so one huge day does not flatten the rest", () => {
    const level = heatLevels([1, 2, 3, 4, 5, 6, 7, 1000]);
    expect(level(0)).toBe(0);
    expect(level(1)).toBe(1);
    expect(level(4)).toBe(2);
    expect(level(6)).toBe(3);
    expect(level(1000)).toBe(4);
  });

  it("ends on today's week, starts on a Monday and never draws a day after today", () => {
    const grid = heatGrid("2026-10-26", 53); // 2026-10-26 是周一
    expect(grid[grid.length - 1]).toEqual({
      day: "2026-10-26",
      col: 52,
      row: 0,
    });
    expect(grid[0].row).toBe(0);
    expect(new Date(2025, 9, 27).getDay()).toBe(1);
    expect(grid[0].day).toBe("2025-10-27");
    const wed = heatGrid("2026-10-28", 2);
    expect(wed[wed.length - 1].day).toBe("2026-10-28");
    expect(wed).toHaveLength(7 + 3);
  });

  it("leaves days before the first record blank, and tells no requests apart from no records", () => {
    const { container } = render(
      <YearHeatmap
        data={[
          { day: "2026-10-20", value: 3, rows: [["站内花费", "$0.10"]] },
          { day: "2026-10-26", value: 9 },
        ]}
        today="2026-10-26"
        firstDay="2026-10-19"
        format={(v) => `${v} 次请求`}
        label="测试"
        weeks={2}
      />,
    );
    // 两周里 10-19 起才有记录：画 8 格（10-19 … 10-26），10-13 … 10-18 不画。
    expect(container.querySelectorAll("rect")).toHaveLength(8);
    expect(container.querySelectorAll("rect.qb-heat-0")).toHaveLength(6);
    const box = screen.getByRole("group");
    fireEvent.focus(box);
    expect(screen.getByText("2026-10-26 周一：9 次请求")).toBeTruthy();
    fireEvent.keyDown(box, { key: "ArrowLeft" });
    expect(screen.getByText("2026-10-19 周一：没有请求")).toBeTruthy();
    fireEvent.keyDown(box, { key: "ArrowDown" });
    expect(screen.getByText("2026-10-20 周二：3 次请求")).toBeTruthy();
    // 往前走到最早记录之前：停在最早画出来的那一格，不读「没有请求」。
    fireEvent.keyDown(box, { key: "Home" });
    expect(screen.getByText("2026-10-19 周一：没有请求")).toBeTruthy();
  });
});

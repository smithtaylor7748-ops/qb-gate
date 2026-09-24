// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import BarChart, { niceTicks } from "./BarChart";

describe("BarChart", () => {
  afterEach(cleanup);

  it("刻度取整，最后一格盖过最大值", () => {
    expect(niceTicks(13.66)).toEqual([0, 5, 10, 15]);
    expect(niceTicks(43.65)).toEqual([0, 20, 40, 60]);
    expect(niceTicks(0.16)).toEqual([0, 0.05, 0.1, 0.15, 0.2]);
    expect(niceTicks(0)).toEqual([0, 1]);
  });

  // 没有记录的那一天不画柱，也不写 $0 —— 那是断言。
  it("没有记录的那一格不画柱子", () => {
    const { container } = render(
      <BarChart
        label="按天"
        format={(v) => `$${v}`}
        data={[
          { key: "a", axis: "09-22", title: "09-22", value: 3 },
          { key: "b", axis: "09-23", title: "09-23", value: null },
          { key: "c", axis: "09-24", title: "09-24", value: 5 },
        ]}
      />,
    );
    expect(container.querySelectorAll("path")).toHaveLength(2);
    expect(container.textContent).toContain("09-23");
  });

  it("一根有值的都没有时给一句话，不画空坐标", () => {
    const { container } = render(
      <BarChart
        label="按天"
        format={(v) => `$${v}`}
        empty="这一档没有记录。"
        data={[{ key: "a", axis: "09-22", title: "09-22", value: null }]}
      />,
    );
    expect(container.querySelector("svg")).toBeNull();
    expect(container.textContent).toContain("这一档没有记录。");
  });
});

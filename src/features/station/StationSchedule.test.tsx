// @vitest-environment jsdom
// 自动选线偏好的三档与排名预览（2026-11-15 起这里不自己存盘：由「选线与分流」弹窗底部那一个「保存」落盘）。
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import type { Prefs } from "../../lib/station";
import { PRESETS, presetOf, RankingPreview } from "./StationSchedule";

const mock = vi.hoisted(() => ({ call: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ call: mock.call }));

const decision = (winner: string | null) => ({
  basis: "unavailable",
  ranking: {
    winner,
    rows: [],
    indistinguishable: [],
    floors_relaxed: false,
    breakers_relaxed: false,
  },
});

beforeEach(() => mock.call.mockReset());
afterEach(cleanup);

it("the saving preset is the backend default: cheap with a 95% success floor", () => {
  const saving = PRESETS.find((p) => p.id === "saving")!;
  expect(saving.name).toBe("省钱");
  expect(saving.prefs).toEqual({
    axes: ["cheap"],
    floors: { min_success_rate: 0.95, max_ttft_p95_ms: null, max_rate: null },
  });
  expect(PRESETS.map((p) => p.name)).toEqual(["省钱", "速度", "稳定"]);
});

it("maps stored preferences to a preset by what they rank on, and says custom otherwise", () => {
  const of = (axes: Prefs["axes"]): Prefs => ({
    axes,
    floors: { min_success_rate: null, max_ttft_p95_ms: null, max_rate: null },
  });
  expect(presetOf(of(["cheap"]))).toBe("saving");
  expect(presetOf(of(["fast"]))).toBe("speed");
  // 顺序不算：稳 + 快 = 快 + 稳。
  expect(presetOf(of(["fast", "stable"]))).toBe("stable");
  expect(presetOf(of(["cheap", "fast"]))).toBeNull();
  expect(presetOf(of([]))).toBeNull();
});

it("the ranking preview ignores an older ranking that finishes after the latest one", async () => {
  let finishOld: ((value: ReturnType<typeof decision>) => void) | undefined;
  mock.call.mockImplementation(async (command, args) => {
    if (command !== "station_decide") return null;
    if (args.prefs.axes[0] === "cheap")
      return new Promise((resolve) => {
        finishOld = resolve;
      });
    return decision("latest-route");
  });
  const saving = PRESETS[0].prefs;
  const speed = PRESETS[1].prefs;
  const view = render(<RankingPreview client="codex" prefs={saving} />);
  await waitFor(() => expect(finishOld).toBeDefined());
  view.rerender(<RankingPreview client="codex" prefs={speed} />);
  await waitFor(() =>
    expect(mock.call).toHaveBeenLastCalledWith("station_decide", {
      prefs: speed,
      client: "codex",
    }),
  );
  await act(async () => finishOld?.(decision("stale-route")));
  expect(screen.queryByText(/stale-route/)).toBeNull();
});

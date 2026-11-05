import { expect, it } from "vitest";

import type { SessionSummary } from "../../lib/sessions";
import { UNOWNED_LABEL, ago, compact, groupSessions } from "./sessionGroups";

function s(over: Partial<SessionSummary>): SessionSummary {
  return {
    id: "x",
    client: "claude-code",
    source: "claude-code:slot:a",
    source_label: "",
    official: true,
    session_id: "s",
    title: "t",
    cwd: null,
    started_at: null,
    updated_at: "2026-11-01T00:00:00Z",
    size: 1,
    path: "",
    kind: "chat",
    project: "",
    tokens: null,
    running: false,
    unowned: false,
    ...over,
  };
}

it("groups by project directory, newest group first, unowned last", () => {
  const groups = groupSessions([
    s({
      id: "1",
      cwd: "D:\\work\\demo",
      project: "demo",
      updated_at: "2026-11-03T00:00:00Z",
    }),
    s({ id: "u", unowned: true, updated_at: "2026-11-04T00:00:00Z" }),
    s({
      id: "2",
      cwd: "D:\\work\\api",
      project: "api",
      updated_at: "2026-11-02T00:00:00Z",
    }),
    s({
      id: "3",
      cwd: "d:\\work\\DEMO",
      project: "DEMO",
      updated_at: "2026-11-01T00:00:00Z",
    }),
    s({ id: "4", cwd: null, project: "" }),
  ]);
  expect(groups.map((g) => [g.label, g.items.map((i) => i.id)])).toEqual([
    ["demo", ["1", "3"]],
    ["api", ["2"]],
    ["（项目目录读不出来）", ["4"]],
    [UNOWNED_LABEL, ["u"]],
  ]);
  expect(groups[groups.length - 1].unowned).toBe(true);
});

it("formats relative times and token counts", () => {
  const now = Date.parse("2026-11-05T12:00:00Z");
  expect(ago("2026-11-05T11:59:30Z", now)).toBe("刚刚");
  expect(ago("2026-11-05T11:30:00Z", now)).toBe("30 分钟前");
  expect(ago("2026-11-03T12:00:00Z", now)).toBe("2 天前");
  expect(compact(1234)).toBe("1.2K");
  expect(compact(2_500_000)).toBe("2.5M");
  expect(compact(12)).toBe("12");
});

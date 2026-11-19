// @vitest-environment jsdom
import { expect, it } from "vitest";
import { demoCall } from "./demo";
import type { RouterSettings } from "./generated/RouterSettings";
import type { RouterStatus } from "./generated/RouterStatus";
import type { ProjectsView } from "./generated/ProjectsView";
import type { Schedule } from "./generated/Schedule";
import type { SetCurrentResult } from "./generated/SetCurrentResult";

const client = "claude-code";
const routerSettings = () =>
  demoCall<RouterSettings>("station_router_settings", { client });
const schedule = async () =>
  (await demoCall<Schedule[]>("station_schedules")).find(
    (s) => s.client === client,
  );

// 2026-11-14 起项目不碰线路：换项目不动备用名单、不关自动选线、不换当前线路（原来这一下会收回成「仅所选」并停调度）。
it("applying a project never touches the backup list, auto-select or the current line", async () => {
  const status = await demoCall<RouterStatus>("station_router_status", {
    client,
  });
  const settings = await routerSettings();
  expect(status.current_route).toBeTruthy();
  await demoCall("station_router_settings_save", {
    settings: {
      ...settings,
      routing_mode: "pool",
      allowed_routes: [status.current_route],
    },
  });
  await demoCall("station_schedule_set", { client, enabled: true });
  const projects = await demoCall<ProjectsView>("projects_create", {
    client,
    name: "Extensions only",
  });
  const created = projects.projects.find((p) => p.name === "Extensions only");
  expect(created).toBeTruthy();
  await demoCall("projects_apply", { client, id: created!.id });
  const after = await routerSettings();
  expect(after.routing_mode).toBe("pool");
  expect(after.allowed_routes).toEqual([status.current_route]);
  expect((await schedule())?.enabled).toBe(true);
  const now = await demoCall<RouterStatus>("station_router_status", { client });
  expect(now.pending_route ?? now.current_route).toBe(status.current_route);
  await demoCall("station_schedule_set", { client, enabled: false });
});

// 「设为当前」只换当前线路：备用名单不动；自动选线开着时关掉并如实返回。
it("setting the current line keeps the backups and reports stopping auto-select", async () => {
  const settings = await routerSettings();
  const routes = await demoCall<{ id: string; client: string }[]>(
    "station_routes",
  );
  const mine = routes.filter((r) => r.client === client).map((r) => r.id);
  expect(mine.length).toBeGreaterThan(1);
  const [first, second] = mine;
  await demoCall("station_set_current", { routeId: first });
  await demoCall("station_router_settings_save", {
    settings: { ...settings, routing_mode: "pool", allowed_routes: [first] },
  });
  await demoCall("station_schedule_set", { client, enabled: true });
  const done = await demoCall<SetCurrentResult>("station_set_current", {
    routeId: second,
  });
  expect(done.auto_select_stopped).toBe(true);
  expect(done.status.selection.current).toBe(second);
  expect(done.status.selection.backups).toEqual([first]);
  expect((await routerSettings()).allowed_routes).toEqual([first]);
  expect((await schedule())?.enabled).toBe(false);
  const again = await demoCall<SetCurrentResult>("station_set_current", {
    routeId: first,
  });
  expect(again.auto_select_stopped).toBe(false);
});

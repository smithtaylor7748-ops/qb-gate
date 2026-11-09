// @vitest-environment jsdom
import { expect, it } from "vitest";
import { demoCall } from "./demo";
import type { RouterSettings } from "./generated/RouterSettings";
import type { RouterStatus } from "./generated/RouterStatus";
import type { ProjectsView } from "./generated/ProjectsView";
import type { Schedule } from "./generated/Schedule";

it("applying a project with the current route leaves the allowed pool and stops scheduling", async () => {
  const client = "claude-code";
  const status = await demoCall<RouterStatus>("station_router_status", {
    client,
  });
  const settings = await demoCall<RouterSettings>("station_router_settings", {
    client,
  });
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
    name: "Explicit same-route selection",
  });
  const selected = projects.projects.find(
    (project) => project.name === "Explicit same-route selection",
  );
  expect(selected).toBeTruthy();
  await demoCall("projects_apply", { client, id: selected!.id });
  expect(
    (await demoCall<RouterSettings>("station_router_settings", { client }))
      .routing_mode,
  ).toBe("selected");
  expect(
    (await demoCall<Schedule[]>("station_schedules")).find(
      (schedule) => schedule.client === client,
    )?.enabled,
  ).toBe(false);
  const after = await demoCall<RouterStatus>("station_router_status", {
    client,
  });
  expect(after.pending_route ?? after.current_route).toBe(status.current_route);
});

import { createServer } from "node:http";
import { once } from "node:events";
import type { AddressInfo } from "node:net";
import { expect, it } from "vitest";
import type { Route } from "../../../lib/generated/Route";
import type { RouterStatus } from "../../../lib/generated/RouterStatus";
import { relayConnection } from "./relayConnection";

const route = (client: "codex" | "gemini-cli", id: string): Route => ({
  id,
  client,
  station_id: "fixture",
  group: "",
  credential_id: "key",
  nominal_rate: null,
  latest_mult: null,
  last_audit_ms: null,
  protocols: { anthropic: false, openai_chat: true, openai_responses: true },
  rates: {
    model_ratio: null,
    completion_ratio: null,
    cache_ratio: null,
    create_cache_ratio: null,
    group_ratio: null,
    peak_rate: null,
    per_request_price: null,
    input_price: null,
    output_price: null,
    cache_read_price: null,
    cache_write_price: null,
  },
  output_markup: null,
  rates_model: null,
  model_map: {
    exact: [],
    default: null,
    haiku: null,
    sonnet: null,
    opus: null,
    fable: null,
    subagent: null,
  },
  upstream_format: null,
});

const status = (base: string, selected: string): RouterStatus => ({
  running: true,
  base_url: base,
  current_route: selected,
  pending_route: null,
  current_by_client: {},
  switches: 0,
  pending_logs: 0,
  turnstate_official_armed: true,
  turnstate_enabled: false,
  turnstate_team: false,
  turnstate_takeover: null,
  breakers: [],
  admission: [],
});

it("keeps SillyTavern chat and model discovery on the selected client route", async () => {
  const paths: string[] = [];
  const server = createServer((request, response) => {
    paths.push(request.url!);
    response.end("{}");
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  try {
    const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
    const routes = [
      route("codex", "gpt-relay"),
      route("gemini-cli", "google-relay"),
    ];
    const gpt = relayConnection("gpt", status(base, "gpt-relay"), routes);
    const google = relayConnection(
      "gemini",
      status(base, "google-relay"),
      routes,
    );
    // SillyTavern appends these API paths to its custom URL / reverse proxy.
    for (const url of [
      `${gpt.url}/models`,
      `${gpt.url}/chat/completions`,
      `${google.url}/v1beta/models/gemini-test:generateContent?key=qb-gate-local-router`,
    ]) {
      await fetch(url);
    }
    expect(paths).toEqual([
      "/codex/v1/models",
      "/codex/v1/chat/completions",
      "/gemini/v1beta/models/gemini-test:generateContent?key=qb-gate-local-router",
    ]);
  } finally {
    server.closeAllConnections();
    await new Promise<void>((resolve, reject) =>
      server.close((e) => (e ? reject(e) : resolve())),
    );
  }
});

it("refuses official accounts, missing keys and incompatible protocols, including a pending switch", () => {
  const router = status("http://127.0.0.1:1234", "gpt-relay");
  const line = route("codex", "gpt-relay");
  expect(relayConnection("gpt", router, [line]).url).not.toBeNull();
  router.pending_route = "qb-router-codex-official";
  expect(relayConnection("gpt", router, [line]).url).toBeNull();
  router.pending_route = null;
  line.credential_id = null;
  expect(relayConnection("gpt", router, [line]).url).toBeNull();
  line.credential_id = "key";
  line.upstream_format = "openai_responses";
  expect(relayConnection("gpt", router, [line]).url).toBeNull();
  router.running = false;
  expect(relayConnection("gpt", router, [line]).url).toBeNull();
});

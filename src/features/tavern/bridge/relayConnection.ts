import type { Route } from "../../../lib/generated/Route";
import type { RouterStatus } from "../../../lib/generated/RouterStatus";
import type { CliProvider } from "./cli/meta";

/** Match a real relay route before offering SillyTavern a local connection. */
export function relayConnection(
  provider: CliProvider,
  router: RouterStatus | null,
  routes: Route[],
): { url: string | null; route: Route | null; problem: string | null } {
  if (!router?.running || !router.base_url) {
    return { url: null, route: null, problem: "本机路由未启动" };
  }
  const client = provider === "gpt" ? "codex" : "gemini-cli";
  const selected = router.pending_route ?? router.current_route;
  const route = routes.find((r) => r.id === selected && r.client === client);
  if (!route?.credential_id) {
    return {
      url: null,
      route: null,
      problem:
        "请先为这个软件选择已配置 API Key 的中转线路；官方账户线路不能用于此连接",
    };
  }
  const compatible =
    provider === "gpt"
      ? (route.upstream_format === null ||
          route.upstream_format === "openai_chat") &&
        route.protocols.openai_chat !== false
      : route.upstream_format === null ||
        route.upstream_format === "gemini_native";
  if (!compatible) {
    return {
      url: null,
      route,
      problem:
        provider === "gpt"
          ? "请选择支持 OpenAI Chat 的中转线路"
          : "请选择 Gemini 原生中转线路",
    };
  }
  // Prefixes route model discovery as well as chat to the chosen client.
  const prefix = provider === "gpt" ? "/codex/v1" : "/gemini";
  return {
    url: `${router.base_url.replace(/\/$/, "")}${prefix}`,
    route,
    problem: null,
  };
}

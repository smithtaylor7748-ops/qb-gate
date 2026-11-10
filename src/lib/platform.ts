import { api } from "./api";
import { res, useResource } from "./store";

const platformResource = res(() => api.platformCapabilities());

/** Capabilities come from the native binary, never from the browser user agent. */
export function usePlatform() {
  return useResource("platform", platformResource).data;
}

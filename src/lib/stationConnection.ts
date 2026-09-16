import type { Credential, Provider } from "./workspace";
import { workspaceApi } from "./workspace";

/** Save the edited address and bind the entered key before saving the route. */
export async function saveStationConnection(
  provider: Provider,
  key: string,
  selected: Credential | undefined,
  group: string,
): Promise<{ stationId: string; credentialId: string }> {
  if (
    !key.trim() &&
    (!selected?.available || selected.provider_id !== provider.id)
  ) {
    throw new Error("请填写 API Key，或选择这个站点已保存的可用凭证");
  }
  const saved = await workspaceApi.saveProvider(provider);
  const credential = key.trim()
    ? await workspaceApi.saveCredential(
        {
          id: "",
          provider_id: saved.id,
          label: group.trim() || "默认分组",
          masked: "",
          available: false,
          revision: 0,
        },
        "replace",
        key.trim(),
      )
    : selected!;
  return { stationId: saved.id, credentialId: credential.id };
}

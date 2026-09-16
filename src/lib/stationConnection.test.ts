import { beforeEach, describe, expect, it, vi } from "vitest";
import { saveStationConnection } from "./stationConnection";
import { workspaceApi, type Provider, type Credential } from "./workspace";

vi.mock("./workspace", () => ({
  workspaceApi: { saveProvider: vi.fn(), saveCredential: vi.fn() },
}));
const provider: Provider = {
  id: "site",
  name: "Site",
  base_url: "https://changed.example/v1",
  website: "",
  note: "updated",
  tags: [],
  favorite: false,
  revision: 4,
};
const credential: Credential = {
  id: "key-a",
  provider_id: "site",
  label: "group",
  masked: "***",
  available: true,
  revision: 1,
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(workspaceApi.saveProvider).mockResolvedValue(provider);
  vi.mocked(workspaceApi.saveCredential).mockResolvedValue(credential);
});
describe("station credential persistence", () => {
  it("saves an entered key and returns its binding, including edited provider fields", async () => {
    expect(
      await saveStationConnection(
        provider,
        "  fixture-key  ",
        undefined,
        "work",
      ),
    ).toEqual({ stationId: "site", credentialId: "key-a" });
    expect(workspaceApi.saveProvider).toHaveBeenCalledWith(provider);
    expect(workspaceApi.saveCredential).toHaveBeenCalledWith(
      expect.objectContaining({ provider_id: "site", label: "work" }),
      "replace",
      "fixture-key",
    );
  });
  it("keeps the existing encrypted credential when the key field is blank", async () => {
    expect(
      await saveStationConnection(provider, "", credential, "work"),
    ).toEqual({ stationId: "site", credentialId: "key-a" });
    expect(workspaceApi.saveCredential).not.toHaveBeenCalled();
  });
  it("rejects a missing, unavailable or other provider's key before mutating data", async () => {
    for (const selected of [
      undefined,
      { ...credential, available: false },
      { ...credential, provider_id: "other" },
    ]) {
      await expect(
        saveStationConnection(provider, "", selected, "work"),
      ).rejects.toThrow("API Key");
    }
    expect(workspaceApi.saveProvider).not.toHaveBeenCalled();
  });
  it("does not report a usable route when encrypting or saving the key fails", async () => {
    vi.mocked(workspaceApi.saveCredential).mockRejectedValue(
      new Error("save failed"),
    );
    await expect(
      saveStationConnection(provider, "fixture-key", undefined, "work"),
    ).rejects.toThrow("save failed");
  });
});

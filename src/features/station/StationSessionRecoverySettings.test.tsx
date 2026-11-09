// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { RouterSettings } from "../../lib/generated/RouterSettings";
import { StationSessionRecoverySettings } from "./StationSessionRecoverySettings";

afterEach(cleanup);

it("offers independent default-off ID and ciphertext controls for Codex", () => {
  const onChange = vi.fn();
  const settings = {
    client: "codex",
    session_recovery: {
      strip_item_ids: false,
      filter_encrypted_content: false,
    },
  } as RouterSettings;
  render(
    <StationSessionRecoverySettings settings={settings} onChange={onChange} />,
  );
  const ids = screen.getByRole("checkbox", { name: "清理历史条目的上游 ID" });
  const encrypted = screen.getByRole("checkbox", {
    name: "过滤无法复用的加密推理状态",
  });
  expect((ids as HTMLInputElement).checked).toBe(false);
  expect((encrypted as HTMLInputElement).checked).toBe(false);
  fireEvent.click(ids);
  expect(onChange).toHaveBeenLastCalledWith({
    session_recovery: { strip_item_ids: true, filter_encrypted_content: false },
  });
  fireEvent.click(encrypted);
  expect(onChange).toHaveBeenLastCalledWith({
    session_recovery: { strip_item_ids: false, filter_encrypted_content: true },
  });
});

it("does not present Codex recovery settings for other clients", () => {
  const settings = { client: "claude-code" } as RouterSettings;
  render(
    <StationSessionRecoverySettings settings={settings} onChange={vi.fn()} />,
  );
  expect(screen.queryByRole("checkbox")).toBeNull();
});

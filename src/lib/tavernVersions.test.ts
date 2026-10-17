import { describe, expect, it } from "vitest";
import { compareVersions, relation } from "./tavernVersions";

describe("tavern versions", () => {
  it("compares segment by segment, not as strings", () => {
    expect(compareVersions("1.13.10", "1.13.9")).toBeGreaterThan(0);
    expect(compareVersions("v1.19.0", "1.19.0")).toBe(0);
    expect(compareVersions("1.18", "1.18.0")).toBe(0);
    expect(compareVersions("1.9.0", "1.10.0")).toBeLessThan(0);
  });

  it("says unknown when nothing is installed rather than guessing", () => {
    expect(relation("1.19.0", null)).toBe("unknown");
    expect(relation("1.19.0", "?")).toBe("unknown");
    expect(relation("1.19.0", "1.18.0")).toBe("newer");
    expect(relation("1.17.0", "1.18.0")).toBe("older");
    expect(relation("1.18.0", "v1.18.0")).toBe("same");
  });
});

import { describe, expect, it } from "vitest";
import { inspectQuality } from "./codexQuality";
const correct = { q1: 25, q2: "B", q3: 64, q4: -10, q5: 4 };
describe("quality evidence", () => {
  it("checks independently solvable constraints", () => {
    expect(
      Array.from({ length: 99 }, (_, i) => i + 1).filter(
        (n) => n % 7 === 4 && n % 5 === 0 && n % 3 === 1,
      ),
    ).toEqual([25]);
    expect(
      ["A", "B", "C"].filter(
        (box) =>
          [box === "B", box === "C", box !== "C"].filter(Boolean).length === 2,
      ),
    ).toEqual(["B"]);
    let x = 1;
    for (let i = 1; i <= 4; i++) x = 2 * x - i;
    expect(x).toBe(-10);
  });
  it("rejects incomplete answers and does not score numbers appearing in explanations", () => {
    expect(() => inspectQuality('{"q1":25}')).toThrow("不完整");
    expect(
      inspectQuality(
        JSON.stringify({ ...correct, q4: "the answer could be 21" }),
      ).failed,
    ).toEqual(["q4"]);
  });
  it("reads final CLI messages while keeping absent usage unknown", () => {
    const raw = [
      { type: "thread.started", thread_id: "example" },
      {
        type: "item.completed",
        item: { type: "agent_message", text: JSON.stringify(correct) },
      },
      { type: "turn.completed", usage: {} },
    ]
      .map((v) => JSON.stringify(v))
      .join("\n");
    expect(inspectQuality(raw)).toMatchObject({
      correct: 5,
      declaredModel: null,
      reasoningTokens: null,
    });
  });
  it("reads response declarations without treating them as model authentication", () => {
    expect(
      inspectQuality(
        JSON.stringify({
          model: "declared-alias",
          status: "completed",
          output_text: JSON.stringify(correct),
          usage: { output_tokens_details: { reasoning_tokens: 0 } },
        }),
      ),
    ).toMatchObject({
      correct: 5,
      declaredModel: "declared-alias",
      reasoningTokens: 0,
    });
  });
  it("reads reasoning token usage from the completed Codex CLI turn", () => {
    const raw = [
      {
        type: "item.completed",
        item: { type: "agent_message", text: JSON.stringify(correct) },
      },
      {
        type: "turn.completed",
        usage: {
          input_tokens: 120,
          output_tokens: 900,
          reasoning_output_tokens: 800,
        },
      },
    ]
      .map((v) => JSON.stringify(v))
      .join("\n");
    expect(inspectQuality(raw)).toMatchObject({
      correct: 5,
      reasoningTokens: 800,
    });
  });
  it("does not score failed or incomplete runs", () => {
    const events = [
      {
        type: "item.completed",
        item: { type: "agent_message", text: JSON.stringify(correct) },
      },
      { type: "turn.failed", error: { message: "interrupted" } },
    ];
    expect(() =>
      inspectQuality(events.map((v) => JSON.stringify(v)).join("\n")),
    ).toThrow("未成功完成");
    expect(() =>
      inspectQuality(
        JSON.stringify({
          status: "incomplete",
          output_text: JSON.stringify(correct),
        }),
      ),
    ).toThrow("未成功完成");
  });
  it("does not attribute a new response to the previous model or usage", () => {
    const events = [
      {
        type: "response.completed",
        response: {
          id: "old",
          status: "completed",
          model: "old-model",
          usage: { output_tokens_details: { reasoning_tokens: 999 } },
          output_text: JSON.stringify(correct),
        },
      },
      { type: "response.created", response: { id: "new" } },
      {
        type: "response.completed",
        response: {
          id: "new",
          status: "completed",
          output_text: JSON.stringify(correct),
        },
      },
    ];
    expect(
      inspectQuality(
        events.map((v) => `data: ${JSON.stringify(v)}`).join("\n\n"),
      ),
    ).toMatchObject({ correct: 5, declaredModel: null, reasoningTokens: null });
  });
});

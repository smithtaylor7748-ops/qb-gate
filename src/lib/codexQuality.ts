/** 独立的小型回归题；结果只证明这些题的表现，不鉴定模型身份。 */
export const QUALITY_SUITE = "qb-reasoning-v1";
const answers = { q1: 25, q2: "B", q3: 64, q4: -10, q5: 4 };
export const qualityPrompt = `完成下面五道独立题，不调用工具或搜索。只输出一个 JSON 对象，键为 q1 到 q5，答案分别为整数或字母；不要附带解释。
q1：一个正整数小于100，除以7余4，除以5余0，除以3余1。它是多少？
q2：只有一个盒子装了硬币。A盒标签说“硬币在B”，B盒标签说“硬币在C”，C盒标签说“硬币不在C”。三个标签恰好两个为真。硬币在A、B、C中的哪个盒子？
q3：有8个独立开关，每个开关只有开或关两种状态。恰好有奇数个开关打开且第一个开关关闭，一共有多少种状态？
q4：令x=1；依次对i=1、2、3、4执行x=2*x-i；最后x等于多少？
q5：四个任务A、B、C、D各耗时1小时，最多同时执行2个任务。B与C必须等A完成才开始，D必须等B与C都完成才开始。另外D开始前必须完整空闲1小时（所有任务都不运行）。从开始到全部完成至少多少小时？`;

export interface QualityResult {
  correct: number;
  total: number;
  failed: string[];
  declaredModel: string | null;
  reasoningTokens: number | null;
}

/** 只接受完整最终答案，不能以正文“出现正确数字”算答对。 */
export function inspectQuality(raw: string): QualityResult {
  if (raw.length > 2_000_000)
    throw new Error("结果过大，请只粘贴这次测试的最终回复或 JSON 输出。");
  const original = raw.trim();
  let text = original;
  let declaredModel: string | null = null;
  let reasoningTokens: number | null = null;
  const inspectResponse = (value: unknown): string | null => {
    if (!value || typeof value !== "object") return null;
    const response = value as Record<string, unknown>;
    if (response.status !== "completed")
      throw new Error(
        "本次响应未成功完成，不能记为通过；请提供完整的已完成结果。",
      );
    // 每个已完成响应独立取证，缺字段不能借用前一响应的数据。
    declaredModel = typeof response.model === "string" ? response.model : null;
    reasoningTokens = null;
    const usage = response.usage as
      | {
          output_tokens_details?: { reasoning_tokens?: unknown };
          reasoning_output_tokens?: unknown;
        }
      | undefined;
    const tokens =
      usage?.output_tokens_details?.reasoning_tokens ??
      usage?.reasoning_output_tokens;
    if (typeof tokens === "number" && Number.isFinite(tokens) && tokens >= 0)
      reasoningTokens = tokens;
    if (typeof response.output_text === "string") return response.output_text;
    if (Array.isArray(response.output)) {
      const messages = response.output.filter(
        (x) => x?.type === "message" && x?.role === "assistant",
      );
      const last = messages[messages.length - 1];
      if (last && Array.isArray(last.content))
        return last.content
          .filter((p: { type?: string }) => p.type === "output_text")
          .map((p: { text?: string }) => p.text ?? "")
          .join("");
    }
    return null;
  };
  let whole: unknown;
  try {
    whole = JSON.parse(text);
  } catch {
    /* 可能是 JSONL/SSE 或代码块 */
  }
  if (
    whole &&
    typeof whole === "object" &&
    !Array.isArray(whole) &&
    "q1" in whole
  ) {
    // 手动粘贴的完整五题答案无需伪造协议事件。
  } else {
    let final: string | null = null;
    let cliCandidate: string | null = null;
    let structured = false;
    let terminalProblem = false;
    const inspectEvent = (value: unknown) => {
      if (!value || typeof value !== "object") return;
      const v = value as Record<string, unknown>;
      const response = v.response as Record<string, unknown> | undefined;
      if (v.type === "response.completed") {
        structured = true;
        final = inspectResponse(response);
        terminalProblem = false;
      } else if (
        v.type === "response.created" ||
        v.type === "response.in_progress" ||
        v.type === "turn.started" ||
        v.type === "thread.started"
      ) {
        structured = true;
        final = null;
        cliCandidate = null;
        declaredModel = null;
        reasoningTokens = null;
        terminalProblem = false;
      } else if (
        v.type === "response.failed" ||
        v.type === "response.incomplete" ||
        v.type === "turn.failed" ||
        v.type === "error"
      ) {
        structured = true;
        final = null;
        terminalProblem = true;
      } else if (v.type === "item.completed") {
        structured = true;
        const item = v.item as { type?: string; text?: unknown } | undefined;
        if (item?.type === "agent_message" && typeof item.text === "string") {
          cliCandidate = item.text;
          final = null;
        }
      } else if (v.type === "turn.completed") {
        structured = true;
        final = cliCandidate;
        declaredModel = null;
        reasoningTokens = null;
        const usage = v.usage as
          { reasoning_output_tokens?: unknown } | undefined;
        if (
          typeof usage?.reasoning_output_tokens === "number" &&
          Number.isFinite(usage.reasoning_output_tokens) &&
          usage.reasoning_output_tokens >= 0
        )
          reasoningTokens = usage.reasoning_output_tokens;
      } else if (
        "output" in v ||
        "output_text" in v ||
        v.object === "response"
      ) {
        structured = true;
        final = inspectResponse(v);
      }
    };
    if (whole) inspectEvent(whole);
    else
      for (const line of original.split(/\r?\n/)) {
        const data = line.startsWith("data:")
          ? line.slice(5).trim()
          : line.trim();
        if (!data || data === "[DONE]") continue;
        let value: unknown;
        try {
          value = JSON.parse(data);
        } catch {
          continue;
        }
        inspectEvent(value);
      }
    if (structured && (terminalProblem || final === null))
      throw new Error(
        "本次响应未成功完成，不能计分；请粘贴包含完成事件的完整输出。",
      );
    if (final !== null) text = final;
  }
  text = text
    .replace(/^```(?:json)?\s*\n?/i, "")
    .replace(/\n?```\s*$/, "")
    .trim();
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error(
      "没有读到完整的最终答案 JSON；请粘贴包含 q1–q5 的最终回复，或 Codex CLI 的 JSONL 输出。",
    );
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
    throw new Error("最终答案必须是包含 q1–q5 的 JSON 对象。");
  const received = parsed as Record<string, unknown>;
  if (
    !Object.keys(answers).every((key) =>
      Object.prototype.hasOwnProperty.call(received, key),
    )
  )
    throw new Error("答案不完整，缺少 q1–q5 中的题目；本次不计分。");
  const failed = Object.entries(answers)
    .filter(([key, expected]) => received[key] !== expected)
    .map(([key]) => key);
  return {
    correct: 5 - failed.length,
    total: 5,
    failed,
    declaredModel,
    reasoningTokens,
  };
}

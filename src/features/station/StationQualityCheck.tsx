import { useEffect, useRef, useState } from "react";
import { Button } from "../../ui";
import {
  qualityApi,
  type QualityCheckResult as SampleResponse,
} from "../../lib/qualityApi";
import {
  animationPrompt,
  candyPrompt,
  inspectAnimationHtml,
  inspectCandyAnswer,
  loadQualityReports,
  plannedQualityRequests,
  QUALITY_CHECK_VERSION,
  sameQualityConditions,
  saveQualityReports,
  type QualityBatchReport,
  type QualityCheckResult,
  type QualityConditions,
  type QualityMethod,
} from "../../lib/qualityChecks";
import {
  createFingerprintPrompts,
  getFingerprintCatalog,
  loadFingerprintReference,
  scoreFingerprint,
} from "../../lib/qualityFingerprint";
import QualityAnimationReview from "./QualityAnimationReview";
import StationQualityLegacy from "./StationQualityLegacy";

const methodLabels: Record<QualityMethod, string> = {
  candy: "糖果题",
  fingerprint: "模型指纹",
  animation: "CSS / SVG 动画",
};
const verdictLabels: Record<QualityCheckResult["verdict"], string> = {
  passed: "通过",
  failed: "未通过",
  inconclusive: "无法判定",
  needs_review: "待人工复核",
  reference: "参考结果",
  error: "请求失败",
  cancelled: "已取消",
};
type ActiveBatch = {
  runId: string;
  targetKey: string;
  requestId: string | null;
  cancelled: boolean;
};
const numberOrNull = (value: number | null | undefined) =>
  typeof value === "number" && Number.isFinite(value) && value >= 0
    ? value
    : null;

export default function StationQualityCheck({
  route = "",
  routeId,
  accountId,
}: {
  route?: string;
  routeId?: string;
  accountId?: string;
}) {
  const catalog = getFingerprintCatalog();
  const target = routeId
    ? { kind: "relay" as const, id: routeId }
    : accountId
      ? { kind: "codex-account" as const, id: accountId }
      : null;
  const targetKey = target ? `${target.kind}:${target.id}` : "";
  const [model, setModel] = useState("");
  const [effort, setEffort] = useState("high");
  const [rounds, setRounds] = useState(1);
  const [candy, setCandy] = useState(true);
  const [fingerprint, setFingerprint] = useState(false);
  const [animation, setAnimation] = useState(false);
  const [reference, setReference] = useState("");
  const [busy, setBusy] = useState(false);
  const [stage, setStage] = useState("");
  const [notice, setNotice] = useState("");
  const [reports, setReports] = useState<QualityBatchReport[]>(() =>
    loadQualityReports(localStorage),
  );
  const reportsRef = useRef(reports);
  const [current, setCurrent] = useState<QualityBatchReport | null>(null);
  const [previews, setPreviews] = useState<Record<number, string>>({});
  const activeRef = useRef<ActiveBatch | null>(null);
  const mounted = useRef(true);
  const methods: QualityMethod[] = [
    candy ? "candy" : null,
    fingerprint ? "fingerprint" : null,
    animation ? "animation" : null,
  ].filter((v): v is QualityMethod => v !== null);
  const planned = plannedQualityRequests(methods, rounds);
  const conditions: QualityConditions = {
    targetId: targetKey,
    targetLabel:
      route ||
      (target?.kind === "codex-account"
        ? "所选官方 Codex 账户"
        : "所选中转线路"),
    model: model.trim(),
    effort,
    transport: target?.kind ?? "relay",
    protocol: null,
    sourceStamp: null,
    suite: QUALITY_CHECK_VERSION,
    fingerprintBank: fingerprint
      ? `${catalog.sdkVersion}/${catalog.dataVersion}/${catalog.referenceSha256}`
      : null,
    fingerprintReference: fingerprint ? reference || model.trim() : null,
    methods,
    rounds,
  };
  // 来源只由本页真实采样结果确认，不能从历史记录推断当前同 ID 的来源。
  if (current?.conditions.targetId === targetKey) {
    conditions.protocol = current.conditions.protocol;
    conditions.sourceStamp = current.conditions.sourceStamp;
  }
  const comparable = reports.filter((report) =>
    sameQualityConditions(report.conditions, conditions),
  );

  const cancel = () => {
    const active = activeRef.current;
    if (!active) return;
    active.cancelled = true;
    setStage("正在取消当前请求，后续请求不会启动…");
    if (active.requestId)
      void qualityApi.cancel(active.requestId).catch(() => {
        if (mounted.current)
          setNotice(
            "取消信号未成功送达；后续请求已停止，当前请求将等待结束或超时。",
          );
      });
  };
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      const active = activeRef.current;
      if (active) {
        active.cancelled = true;
        if (active.requestId)
          void qualityApi.cancel(active.requestId).catch(() => {});
      }
    };
  }, []);
  useEffect(() => {
    if (activeRef.current && activeRef.current.targetKey !== targetKey)
      cancel();
  }, [targetKey]);

  const publish = (report: QualityBatchReport) => {
    const next = [
      report,
      ...[...reportsRef.current, ...loadQualityReports(localStorage)].filter(
        (r, index, all) =>
          r.id !== report.id &&
          all.findIndex((candidate) => candidate.id === r.id) === index,
      ),
    ].slice(0, 30);
    reportsRef.current = next;
    if (mounted.current) {
      setReports(next);
      setCurrent(report);
    }
    try {
      saveQualityReports(localStorage, next);
    } catch {
      if (mounted.current)
        setNotice(
          "本地记录保存失败，本轮结果仍显示在页面上。请检查浏览器存储空间。",
        );
    }
  };

  const start = async () => {
    if (!target || !model.trim() || !planned || activeRef.current) return;
    const chosenTarget = { ...target };
    const active: ActiveBatch = {
      runId: crypto.randomUUID(),
      targetKey,
      requestId: null,
      cancelled: false,
    };
    activeRef.current = active;
    const report: QualityBatchReport = {
      version: 2,
      id: active.runId,
      at: new Date().toISOString(),
      conditions: {
        ...conditions,
        protocol: null,
        sourceStamp: null,
        methods: [...methods],
      },
      plannedRequests: planned,
      attemptedRequests: 0,
      state: "completed",
      results: [],
      samples: [],
    };
    setBusy(true);
    setNotice("");
    setPreviews({});
    setCurrent(report);
    let sampledSourceStamp: string | null | undefined;
    const refresh = () => {
      if (mounted.current)
        setCurrent({
          ...report,
          results: [...report.results],
          samples: [...report.samples],
        });
    };
    const addResult = (result: QualityCheckResult) => {
      report.results.push(result);
      refresh();
    };
    const sample = async (
      method: QualityMethod,
      round: number,
      index: number,
      prompt: string,
      maxOutput: number,
    ): Promise<SampleResponse | null> => {
      if (active.cancelled) return null;
      const requestId = crypto.randomUUID();
      active.requestId = requestId;
      report.attemptedRequests++;
      if (mounted.current)
        setStage(
          `第 ${round}/${rounds} 轮 · ${methodLabels[method]}${method === "fingerprint" ? `样本 ${index}/3` : ""} · 请求 ${report.attemptedRequests}/${planned}`,
        );
      refresh();
      const began = Date.now();
      try {
        const result = await qualityApi.run({
          request_id: requestId,
          run_id: active.runId,
          target: chosenTarget,
          model: report.conditions.model,
          effort,
          prompt,
          max_output_tokens: maxOutput,
          timeout_secs: 180,
        });
        const sourceStamp =
          typeof result.source_stamp === "string" &&
          result.source_stamp.length > 0
            ? result.source_stamp
            : null;
        if (
          result.request_id !== requestId ||
          result.target_id !== chosenTarget.id ||
          result.target_kind !== chosenTarget.kind ||
          result.requested_model !== report.conditions.model ||
          (!active.cancelled &&
            result.status !== "cancelled" &&
            ((sampledSourceStamp !== undefined &&
              sampledSourceStamp !== sourceStamp) ||
              (report.conditions.protocol !== null &&
                report.conditions.protocol !== result.protocol)))
        ) {
          report.state = "error";
          active.cancelled = true;
          throw new Error("检测结果的目标与本批次不一致，已停止后续请求。");
        }
        sampledSourceStamp = sourceStamp;
        if (!active.cancelled && result.status !== "cancelled") {
          if (result.target_label && result.target_label !== result.target_id)
            report.conditions.targetLabel = result.target_label;
          report.conditions.protocol ??= result.protocol;
          report.conditions.sourceStamp = sourceStamp;
        }
        report.samples.push({
          method,
          round,
          index,
          status:
            active.cancelled || result.status === "cancelled"
              ? "cancelled"
              : result.status === "completed"
                ? "completed"
                : "error",
          elapsedMs: result.elapsed_ms,
          declaredModel: result.actual_model,
          inputTokens: numberOrNull(result.input_tokens),
          outputTokens: numberOrNull(result.output_tokens),
          reasoningTokens: numberOrNull(result.reasoning_tokens),
          cacheReadTokens: numberOrNull(result.cache_read_tokens),
          firstTokenMs: numberOrNull(result.first_token_ms),
          protocol: result.protocol,
        });
        if (
          !result.output_limit_enforced &&
          mounted.current &&
          !active.cancelled
        )
          setNotice(
            "本批次通过官方 Codex CLI 采样，输出 Token 上限无法强制执行；每次请求采用 180 秒超时和输出大小限制。",
          );
        if (active.cancelled || result.status === "cancelled") {
          active.cancelled = true;
          addResult({
            method,
            round,
            verdict: "cancelled",
            summary: "本项已取消，未对返回内容计分。",
          });
          return null;
        }
        if (result.status !== "completed") {
          if (result.error && mounted.current)
            setNotice(result.error.slice(0, 1200));
          addResult({
            method,
            round,
            verdict: "error",
            summary:
              result.status === "timeout"
                ? "请求超时，未计分。"
                : "上游未成功完成请求，未计分。",
          });
          return null;
        }
        return result;
      } catch (error) {
        if (mounted.current && !active.cancelled)
          setNotice(
            error instanceof Error
              ? error.message.slice(0, 1200)
              : "检测请求失败，请检查所选账户或线路设置。",
          );
        report.samples.push({
          method,
          round,
          index,
          status:
            report.state === "error"
              ? "error"
              : active.cancelled
                ? "cancelled"
                : "error",
          elapsedMs: Date.now() - began,
          declaredModel: null,
          inputTokens: null,
          outputTokens: null,
          reasoningTokens: null,
          cacheReadTokens: null,
          firstTokenMs: null,
          protocol: null,
        });
        addResult({
          method,
          round,
          verdict:
            report.state === "error"
              ? "error"
              : active.cancelled
                ? "cancelled"
                : "error",
          summary:
            report.state === "error"
              ? "检测目标、来源或协议不一致，已停止本批次。"
              : active.cancelled
                ? "本项已取消，未计分。"
                : "检测请求未成功完成，未计分。",
        });
        return null;
      } finally {
        active.requestId = null;
        refresh();
      }
    };
    try {
      if (fingerprint) {
        setStage("正在校验本地指纹参考数据…");
        try {
          await loadFingerprintReference();
        } catch {
          report.state = "error";
          addResult({
            method: "fingerprint",
            round: 1,
            verdict: "inconclusive",
            summary: "内置指纹参考数据校验失败，本批次未发送模型请求。",
          });
          return;
        }
      }
      for (let round = 1; round <= rounds && !active.cancelled; round++) {
        if (candy) {
          const response = await sample("candy", round, 1, candyPrompt, 2048);
          if (response)
            addResult({ ...inspectCandyAnswer(response.text), round });
        }
        if (fingerprint && !active.cancelled) {
          const outputs: { text: string; expected_count: number }[] = [];
          const challenges = createFingerprintPrompts(
            `${active.runId}:${round}`,
          );
          for (const [index, challenge] of challenges.entries()) {
            const response = await sample(
              "fingerprint",
              round,
              index + 1,
              challenge.prompt,
              4096,
            );
            if (!response) break;
            outputs.push({
              text: response.text,
              expected_count: challenge.expected_count,
            });
          }
          if (!active.cancelled && outputs.length === 3) {
            if (mounted.current)
              setStage(`第 ${round}/${rounds} 轮 · 正在本地计算指纹参考结果…`);
            try {
              const score = await scoreFingerprint(
                outputs,
                report.conditions.fingerprintReference ??
                  report.conditions.model,
              );
              if (!active.cancelled)
                addResult({
                  method: "fingerprint",
                  round,
                  verdict:
                    score.verdict === "insufficient" ||
                    score.verdict === "unknown_reference"
                      ? "inconclusive"
                      : "reference",
                  summary: score.summary,
                  topModel:
                    score.analysis.prediction_name ??
                    score.analysis.prediction ??
                    undefined,
                  probability:
                    numberOrNull(score.analysis.probability) ?? undefined,
                });
            } catch {
              if (!active.cancelled)
                addResult({
                  method: "fingerprint",
                  round,
                  verdict: "inconclusive",
                  summary:
                    "本地指纹分析未完成；请检查内置参考数据，未新增采样请求。",
                });
            }
          }
        }
        if (animation && !active.cancelled) {
          const response = await sample(
            "animation",
            round,
            1,
            animationPrompt,
            8192,
          );
          if (response) {
            const inspection = inspectAnimationHtml(response.text);
            addResult({
              method: "animation",
              round,
              verdict: inspection.ok ? "needs_review" : "failed",
              summary: inspection.ok
                ? "静态格式检查通过，仍需人工观察动画。"
                : inspection.issues.join(" "),
            });
            if (inspection.html && mounted.current)
              setPreviews((v) => ({ ...v, [round]: inspection.html! }));
          }
        }
      }
      if (active.cancelled && report.state !== "error")
        report.state = "cancelled";
      else if (report.samples.some((s) => s.status === "error"))
        report.state = "error";
    } catch {
      report.state = "error";
      if (mounted.current) setNotice("本批次未能继续，已保留完成的分项记录。");
    } finally {
      const final = {
        ...report,
        results: [...report.results],
        samples: [...report.samples],
      };
      publish(final);
      activeRef.current = null;
      if (mounted.current) {
        setBusy(false);
        setStage(
          report.state === "cancelled"
            ? "批次已取消"
            : report.state === "error"
              ? "批次结束，部分请求未完成"
              : "批次完成",
        );
      }
    }
  };

  const recordReview = (
    round: number,
    verdict: "passed" | "failed",
    review: Record<string, boolean>,
  ) => {
    if (!current || busy) return;
    publish({
      ...current,
      results: current.results.map((result) =>
        result.method === "animation" && result.round === round
          ? {
              ...result,
              verdict,
              review,
              summary:
                verdict === "passed"
                  ? "人工复核通过；仅表示本次动画符合列出的要求。"
                  : "人工复核未通过；保留逐项观察结果。",
            }
          : result,
      ),
    });
  };
  const completed =
    current?.samples.filter((s) => s.status === "completed").length ?? 0;
  return (
    <div className="qb-st-form">
      <p className="qb-st-note">
        在明确选中的线路或账户上运行独立新对话，观察单题表现、模型指纹与动画能力。各项分别记录；少量测试不能证明模型身份或整体“降智”。
      </p>
      <h4 className="qb-st-sect">检测对象与条件</h4>
      <p>
        <strong>
          {target?.kind === "codex-account" ? "官方 Codex 账户" : "中转线路"}：
        </strong>
        {route || "尚未选择"}
      </p>
      {!target && (
        <p className="qb-st-nudge">
          请先明确选择一条 Codex
          中转线路或一个官方账户，才能自动检测。也可以展开下方高级手动导入。
        </p>
      )}
      <label className="qb-st-fieldrow">
        模型
        <input
          aria-label="自动检测模型"
          list="qb-quality-models"
          disabled={busy}
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="填写此线路或账户可用的模型 ID"
        />
      </label>
      <datalist id="qb-quality-models">
        {catalog.models.map((m) => (
          <option key={m.id} value={m.id}>
            {m.label}
          </option>
        ))}
      </datalist>
      <label className="qb-st-fieldrow">
        推理强度
        <select
          aria-label="自动检测推理强度"
          disabled={busy}
          value={effort}
          onChange={(e) => setEffort(e.target.value)}
        >
          {[
            "none",
            "minimal",
            "low",
            "medium",
            "high",
            "xhigh",
            "max",
            "ultra",
          ].map((value) => (
            <option key={value}>{value}</option>
          ))}
        </select>
      </label>
      <p className="qb-st-note">
        模型与推理强度必须由所选上游支持，不支持会记录为请求失败。请求不带已有聊天历史，期间不自动切换线路。
      </p>
      <h4 className="qb-st-sect">选择检测方法</h4>
      <label className="qb-st-check">
        <input
          type="checkbox"
          checked={candy}
          disabled={busy}
          onChange={(e) => setCandy(e.target.checked)}
        />{" "}
        快速糖果题 · 每轮 1 请求
      </label>
      <p className="qb-st-note">
        按形状挑选的保证问题；严格读取最终整数
        JSON，格式不符会记录为无法判定。通过只说明这一题答对。
      </p>
      <label className="qb-st-check">
        <input
          type="checkbox"
          checked={fingerprint}
          disabled={busy}
          onChange={(e) => setFingerprint(e.target.checked)}
        />{" "}
        模型指纹（实验性）· 每轮 3 请求
      </label>
      {fingerprint && (
        <>
          <label className="qb-st-fieldrow">
            预期参考类
            <select
              aria-label="指纹预期参考类"
              value={reference}
              disabled={busy}
              onChange={(e) => setReference(e.target.value)}
            >
              <option value="">按填写的模型 ID 匹配</option>
              {catalog.models.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                  {m.merged ? "（合并类）" : ""}
                </option>
              ))}
            </select>
          </label>
          <p className="qb-st-note">
            固定库共 {catalog.models.length} 类，版本 {catalog.dataVersion}
            。结果为库内相对概率，不包含库外模型概率，也不能认证身份。合并参考类不能区分其成员；未知模型和样本不足均为无法判定。
          </p>
        </>
      )}
      <details>
        <summary>进阶：CSS / SVG 动画与人工复核</summary>
        <label className="qb-st-check">
          <input
            type="checkbox"
            checked={animation}
            disabled={busy}
            onChange={(e) => setAnimation(e.target.checked)}
          />{" "}
          灯塔与小船动画 · 每轮 1 请求
        </label>
        <p className="qb-st-note">
          检查格式后在禁止脚本和联网的隔离预览中观看，需要逐项人工复核。静态检查通过不等于动画通过。
        </p>
      </details>
      <label className="qb-st-fieldrow">
        重复轮数
        <select
          aria-label="检测轮数"
          disabled={busy}
          value={rounds}
          onChange={(e) => setRounds(Number(e.target.value))}
        >
          {[1, 2, 3].map((n) => (
            <option key={n} value={n}>
              {n} 轮
            </option>
          ))}
        </select>
      </label>
      <p className="qb-st-note">
        本批计划 <strong>{planned} 次请求</strong>
        ，串行执行，不自动重试；每次最长 180
        秒。会消耗所选目标的额度或费用，金额取决于实际用量与上游计价。
      </p>
      {target?.kind === "codex-account" && (
        <p className="qb-st-note">
          官方账户通过 Codex CLI 临时会话运行，包含 CLI 自身上下文，不能与中转
          API 结果直接等同。CLI 无法强制输出 Token
          上限，采用超时与输出大小限制。
        </p>
      )}
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <Button
          disabled={busy || !target || !model.trim() || !planned}
          onClick={() => void start()}
        >
          开始自动检测（{planned} 次请求）
        </Button>
        {busy && <Button onClick={cancel}>取消本批次</Button>}
      </div>
      {stage && (
        <p role="status" className="qb-st-note">
          {stage}
        </p>
      )}
      {notice && (
        <p role="status" className="qb-st-note">
          {notice}
        </p>
      )}
      {current && (
        <>
          <h4 className="qb-st-sect">
            本批结果 · {current.conditions.targetLabel}
          </h4>
          <progress
            aria-label="检测请求进度"
            max={current.plannedRequests || 1}
            value={current.samples.length}
          />
          <p className="qb-st-note">
            已结束 {current.samples.length}/{current.plannedRequests}
            ，其中成功响应 {completed}；请求失败和取消独立统计，不计为答错。
          </p>
          <div className="qb-st-tw">
            <table>
              <thead>
                <tr>
                  <th>轮次 / 方法</th>
                  <th>结果</th>
                  <th>证据</th>
                </tr>
              </thead>
              <tbody>
                {current.results.map((result, index) => (
                  <tr key={`${result.method}-${result.round}-${index}`}>
                    <td>
                      {result.round} · {methodLabels[result.method]}
                    </td>
                    <td>{verdictLabels[result.verdict]}</td>
                    <td>
                      {result.summary}
                      {result.probability !== undefined && (
                        <>
                          <br />
                          库内相对概率 {(result.probability * 100).toFixed(1)}%
                        </>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <details>
            <summary>逐次请求：用量、耗时与返回模型声明</summary>
            <div className="qb-st-tw">
              <table>
                <thead>
                  <tr>
                    <th>请求</th>
                    <th>状态 / 耗时</th>
                    <th>输入 / 输出 / 推理 / 缓存 Token</th>
                    <th>首 Token / 协议 / 模型声明</th>
                  </tr>
                </thead>
                <tbody>
                  {current.samples.map((s, i) => (
                    <tr key={i}>
                      <td>
                        {s.round} · {methodLabels[s.method]} · {s.index}
                      </td>
                      <td>
                        {s.status === "completed"
                          ? "完成"
                          : s.status === "cancelled"
                            ? "取消"
                            : "失败"}{" "}
                        / {(s.elapsedMs / 1000).toFixed(1)} 秒
                      </td>
                      <td>
                        {s.inputTokens ?? "未知"} / {s.outputTokens ?? "未知"} /{" "}
                        {s.reasoningTokens ?? "未知"} /{" "}
                        {s.cacheReadTokens ?? "未知"}
                      </td>
                      <td>
                        {s.firstTokenMs === null
                          ? "未知"
                          : `${(s.firstTokenMs / 1000).toFixed(2)} 秒`}{" "}
                        / {s.protocol ?? "未知"}
                        <br />
                        {s.declaredModel ?? "未提供"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="qb-st-note">
              模型名是上游声明；耗时包含等待和生成，未作为纯解码速度。官方 CLI
              不提供逐 Token 流，首 Token 时间保持未知。
            </p>
          </details>
          {!busy &&
            Object.entries(previews).map(([round, html]) => (
              <details key={`${current.id}-${round}`}>
                <summary>第 {round} 轮动画预览与人工复核</summary>
                <QualityAnimationReview
                  html={html}
                  onReview={(verdict, review) =>
                    recordReview(Number(round), verdict, review)
                  }
                />
              </details>
            ))}
        </>
      )}
      <h4 className="qb-st-sect">本地批次记录</h4>
      <p className="qb-st-note">
        同条件已记录 {comparable.length} 批。
        {conditions.sourceStamp
          ? "以本页最近实际检测返回的来源和协议为准；来源、模型、推理强度、方法、轮数及参考库版本均相同才比较。"
          : "来源尚未核验，运行一批检测后才能确认历史是否可比。"}
        重启应用后需要重新检测，旧来源标记不跨重启比较。官方账户标记只核对调用环境，不核验登录身份。仅保存分项结果、用量和条件；回复正文与动画仅留在本轮内存。
      </p>
      {reports.length > 0 && (
        <div className="qb-st-tw">
          <table>
            <thead>
              <tr>
                <th>时间 / 条件</th>
                <th>完成情况</th>
                <th>分项结果</th>
              </tr>
            </thead>
            <tbody>
              {reports.map((report) => (
                <tr key={report.id}>
                  <td>
                    {new Date(report.at).toLocaleString()}
                    <br />
                    {report.conditions.targetLabel} · {report.conditions.model}{" "}
                    · {report.conditions.effort}
                    <br />
                    {!conditions.sourceStamp
                      ? "来源尚未核验"
                      : !report.conditions.sourceStamp
                        ? "历史来源未知"
                        : sameQualityConditions(report.conditions, conditions)
                          ? "同条件"
                          : "不同来源或条件"}{" "}
                    · {report.conditions.rounds} 轮 ·{" "}
                    {report.conditions.protocol ?? "协议未知"}
                  </td>
                  <td>
                    {
                      report.samples.filter((s) => s.status === "completed")
                        .length
                    }
                    /{report.plannedRequests} 成功响应
                    <br />
                    {report.state === "completed"
                      ? "批次完成"
                      : report.state === "cancelled"
                        ? "已取消"
                        : "包含失败"}
                  </td>
                  <td>
                    {report.results.map((r, i) => (
                      <div key={i}>
                        {r.round} · {methodLabels[r.method]}：
                        {verdictLabels[r.verdict]}
                        {r.answer !== undefined ? `（${r.answer}）` : ""}
                        {r.topModel ? ` · ${r.topModel}` : ""}
                        {r.probability !== undefined
                          ? ` · ${(r.probability * 100).toFixed(1)}%（库内）`
                          : ""}
                      </div>
                    ))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <details>
        <summary>高级：原五题手动导入与历史记录</summary>
        <StationQualityLegacy route={route} />
      </details>
    </div>
  );
}

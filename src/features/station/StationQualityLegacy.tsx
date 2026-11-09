import { useEffect, useState } from "react";
import { Button } from "../../ui";
import { stationApi } from "../../lib/station";
import {
  inspectQuality,
  qualityPrompt,
  QUALITY_SUITE,
  type QualityResult,
} from "../../lib/codexQuality";

type Run = QualityResult & {
  at: string;
  model: string;
  effort: string;
  route: string;
};
const storageKey = "qb-quality-check-v1";
function loadRuns(): Run[] {
  try {
    const parsed: unknown = JSON.parse(
      localStorage.getItem(storageKey) ?? "[]",
    );
    return Array.isArray(parsed)
      ? parsed
          .filter(
            (r) =>
              r &&
              typeof r.at === "string" &&
              typeof r.model === "string" &&
              typeof r.effort === "string" &&
              typeof r.route === "string" &&
              Number.isInteger(r.correct) &&
              r.correct >= 0 &&
              r.correct <= 5 &&
              r.total === 5 &&
              Array.isArray(r.failed),
          )
          .slice(0, 30)
      : [];
  } catch {
    return [];
  }
}

export default function StationQualityLegacy({
  route = "",
}: {
  route?: string;
}) {
  const [model, setModel] = useState("");
  const [effort, setEffort] = useState("high");
  const [source, setSource] = useState(route);
  const [raw, setRaw] = useState("");
  const [runs, setRuns] = useState<Run[]>(loadRuns);
  const [notice, setNotice] = useState("");
  const [legacy, setLegacy] = useState(false);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let active = true;
    stationApi
      .routerStatus("codex")
      .then((s) => {
        if (active)
          setLegacy(
            s.turnstate_official_armed ||
              s.turnstate_takeover !== null ||
              s.turnstate_enabled,
          );
      })
      .catch((e: unknown) => {
        if (active) setNotice(`旧配置状态读取失败：${String(e)}`);
      });
    return () => {
      active = false;
    };
  }, []);
  const save = () => {
    try {
      if (!model.trim() || !source.trim())
        throw new Error(
          "先填写本次使用的模型和线路，才能与相同条件的历史结果比较。",
        );
      const result = inspectQuality(raw);
      const next = [
        {
          ...result,
          at: new Date().toISOString(),
          model: model.trim(),
          effort,
          route: source.trim(),
        },
        ...runs,
      ].slice(0, 30);
      localStorage.setItem(storageKey, JSON.stringify(next));
      setRuns(next);
      setRaw("");
      setNotice(
        `本次 ${result.correct}/5；${result.failed.length ? `未通过 ${result.failed.join("、")}` : "五题均通过"}。这只是固定题组表现。`,
      );
    } catch (e) {
      setNotice(e instanceof Error ? e.message : String(e));
    }
  };
  const restore = async () => {
    setBusy(true);
    try {
      await stationApi.turnstateDisable();
      setLegacy(false);
      setNotice(
        "旧 turn-state 接管已恢复。已使用该入口的 Codex 请自行重启后继续。",
      );
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBusy(false);
    }
  };
  const comparable = runs.filter(
    (r) =>
      r.model === model.trim() &&
      r.effort === effort &&
      r.route === source.trim(),
  );
  return (
    <div className="qb-st-form">
      <p className="qb-st-note">
        原 turn-state 长度筛选与注入已停用。292 / 332
        等长度不能判断模型质量。这里通过同模型、同推理强度、同线路的多次结果观察变化；少量题目无法证明或排除整体“降智”。
      </p>
      {legacy && (
        <div className="qb-st-nudge">
          <p>
            检测到旧 turn-state 接管。恢复会撤销旧路由配置；使用该入口的 Codex
            需自行重启。
          </p>
          <Button disabled={busy} onClick={() => void restore()}>
            恢复旧 turn-state 配置
          </Button>
        </div>
      )}
      <h4 className="qb-st-sect">1 · 记录测试条件</h4>
      <label className="qb-st-fieldrow">
        模型
        <input
          aria-label="测试模型"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="填写客户端实际选择的模型"
        />
      </label>
      <label className="qb-st-fieldrow">
        推理强度
        <select
          aria-label="测试推理强度"
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
          ].map((x) => (
            <option key={x}>{x}</option>
          ))}
        </select>
      </label>
      <label className="qb-st-fieldrow">
        线路 / 账户标签
        <input
          aria-label="测试线路"
          value={source}
          onChange={(e) => setSource(e.target.value)}
          placeholder="只填名称，不填 Key"
        />
      </label>
      <h4 className="qb-st-sect">2 · 在要检查的客户端新建对话测试</h4>
      <p className="qb-st-note">
        复制这组五题到要检查的 GPT / Codex。建议每种条件至少做 3
        次独立新对话，保持工具、提示词和推理强度一致。正常发送会消耗所选账户额度；手动导入不会发送请求。
      </p>
      <Button
        onClick={() =>
          void navigator.clipboard.writeText(qualityPrompt).then(
            () => setNotice("测试题已复制。"),
            (e) => setNotice(`复制失败：${String(e)}`),
          )
        }
      >
        复制五题测试
      </Button>
      <details>
        <summary>查看测试题（{QUALITY_SUITE}）</summary>
        <pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>
          {qualityPrompt}
        </pre>
      </details>
      <h4 className="qb-st-sect">3 · 检查最终回复</h4>
      <textarea
        aria-label="测试结果"
        rows={6}
        value={raw}
        onChange={(e) => setRaw(e.target.value)}
        placeholder="粘贴包含 q1–q5 的最终答案 JSON，也支持 Responses JSON/SSE 或 Codex exec --json 的完整输出。"
      />
      <p className="qb-st-note">
        只保存分数、时间和测试标签，不保存回复正文。返回模型名属于上游声明；一致不证明模型身份，别名不同也不等于降级。推理
        token 缺失会保持未知，不按 0 计算。
      </p>
      <Button disabled={!raw.trim()} onClick={save}>
        检查并记录
      </Button>
      {notice && (
        <p role="status" className="qb-st-note">
          {notice}
        </p>
      )}
      <p className="qb-st-note">
        相同条件已记录 {comparable.length} 次
        {comparable.length
          ? `，合计通过 ${comparable.reduce((n, r) => n + r.correct, 0)} / ${comparable.length * 5} 题`
          : ""}
        。反复答错只提示进一步复测，不能据此认定上游换了模型。
      </p>
      {runs.length > 0 && (
        <>
          <div className="qb-st-tw">
            <table>
              <thead>
                <tr>
                  <th>时间</th>
                  <th>测试条件</th>
                  <th>题目通过</th>
                  <th>返回声明 / 推理 token</th>
                </tr>
              </thead>
              <tbody>
                {runs.map((r, i) => (
                  <tr key={`${r.at}-${i}`}>
                    <td>{new Date(r.at).toLocaleString()}</td>
                    <td>
                      {r.model} · {r.effort}
                      <br />
                      {r.route}
                    </td>
                    <td>{r.correct}/5</td>
                    <td>
                      {r.declaredModel ?? "未提供"} /{" "}
                      {r.reasoningTokens ?? "未知"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <Button
            onClick={() => {
              try {
                localStorage.removeItem(storageKey);
                setRuns([]);
                setNotice("已清除原五题测试记录。");
              } catch {
                setNotice("无法清除本地测试记录，请检查存储权限。");
              }
            }}
          >
            清除测试记录
          </Button>
        </>
      )}
    </div>
  );
}

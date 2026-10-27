/**
 * 站点的用量脚本编辑器（中转站重构阶段 6 · U4，照 cc-switch 的 `UsageScriptModal`）。从站点卡表头那一格的「脚本」打开。
 *
 * 一段 JS，求值成 `({ request: {...}, extractor: function (response) {...} })`：面板发 `request`、把回复交给 `extractor`，
 * 它返回 `{ remaining, total?, used?, unit?, planName?, isValid?, invalidMessage? }`（或一组）。开着的话表头那一格就用它。
 *
 * ⛔ 跟 cc-switch 不一样的地方要写在界面上：请求地址必须跟站点同源、而且是 https，没有「自定义模板」的例外；
 * Key 用 `{{apiKey}}`，只在跑的那一刻换进去 —— 别把 Key 直接写进脚本（脚本存在面板的库里，不加密）。
 */
import { useEffect, useState } from "react";

import type { SiteQuota } from "../../lib/generated/SiteQuota";
import { stationApi } from "../../lib/station";
import { Button, Modal, useToast } from "../../ui";
import { quotaLine } from "./StationSiteQuota";

/** 两份起手的模板。名字只是名字 —— 跑起来照样要同源、https。 */
export const SCRIPT_TEMPLATES: { id: string; name: string; code: string }[] = [
  {
    id: "newapi-key",
    name: "New API / One API：这把 Key 的剩余额度",
    code: `({
  // New API、One API 系的站点给 Key 留了一个 OpenAI 兼容的账单接口。
  // hard_limit_usd 是这把 Key 还能用的额度（不限额的 Key 会显示一个很大的数）。
  request: {
    url: "{{origin}}/v1/dashboard/billing/subscription",
    method: "GET",
    headers: { Authorization: "Bearer {{apiKey}}" }
  },
  extractor: function (response) {
    return {
      planName: "这把 Key 的额度",
      remaining: response.hard_limit_usd,
      unit: "USD"
    };
  }
})
`,
  },
  {
    id: "blank",
    name: "空白模板（照着改）",
    code: `({
  request: {
    // 只能是这个站点自己的地址（同源、https）；{{origin}} = 协议 + 主机 + 端口，{{baseUrl}} = 站点地址。
    url: "{{origin}}/api/your/balance",
    method: "GET",
    headers: { Authorization: "Bearer {{apiKey}}" }
  },
  extractor: function (response) {
    // 返回 remaining（必填）、total、used、unit、planName、isValid、invalidMessage；也可以返回一组。
    return { remaining: response.balance, unit: "CNY" };
  }
})
`,
  },
];

export default function SiteUsageScript({
  stationId,
  routeId,
  siteName,
  onClose,
  onSaved,
}: {
  stationId: string;
  /** 试跑用哪条线路的 Key（站点在这个软件下的第一条）。 */
  routeId: string;
  siteName: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const toast = useToast();
  const [code, setCode] = useState("");
  const [enabled, setEnabled] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<SiteQuota | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let live = true;
    stationApi
      .usageScript(stationId)
      .then((s) => {
        if (!live) return;
        setCode(s.code || SCRIPT_TEMPLATES[0].code);
        setEnabled(s.enabled);
        setLoaded(true);
      })
      .catch((e: unknown) => {
        if (live) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      live = false;
    };
  }, [stationId]);

  const test = async () => {
    setBusy(true);
    setError("");
    setResult(null);
    try {
      setResult(await stationApi.testUsageScript(routeId, code));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const save = async () => {
    setBusy(true);
    setError("");
    try {
      await stationApi.saveUsageScript(stationId, enabled, code);
      toast.ok(
        enabled ? "存好了：站点卡表头那一格改用这段脚本" : "存好了（没开）",
      );
      onSaved();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="huge"
      title={`用量脚本 · ${siteName}`}
      footer={
        <>
          <Button onClick={onClose}>取消</Button>
          <Button loading={busy} disabled={!loaded} onClick={() => void test()}>
            用这条线路的 Key 试跑一次
          </Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!loaded}
            onClick={() => void save()}
          >
            存
          </Button>
        </>
      }
    >
      <div className="qb-session" data-testid="usage-script">
        <p className="qb-muted">
          给这个站点写一段 JS：面板发 <code>request</code>，把回复交给{" "}
          <code>extractor</code>
          ，它返回剩多少。开着的话站点卡表头那一格就用它（比内置认得的那几家优先）。脚本在沙箱里跑：
          只能算，不能联网、不能读文件，最多 16 MiB 内存、5 秒。
        </p>
        <p className="notice">
          请求地址<strong>必须跟这个站点同源、而且是 https</strong>
          ，发到别处的一概拒绝 —— Key 不会被一段脚本带走。Key 用
          <code>{"{{apiKey}}"}</code>，只在跑的那一刻换进去；别把 Key
          直接写进脚本（脚本存在面板的库里，不加密）。
        </p>
        <div className="qb-filter-pills" role="group" aria-label="模板">
          {SCRIPT_TEMPLATES.map((t) => (
            <button
              key={t.id}
              type="button"
              aria-pressed={code === t.code}
              disabled={!loaded}
              onClick={() => setCode(t.code)}
            >
              {t.name}
            </button>
          ))}
        </div>
        <textarea
          className="input qb-code-input"
          rows={16}
          spellCheck={false}
          aria-label="脚本"
          value={code}
          disabled={!loaded}
          onChange={(e) => setCode(e.target.value)}
        />
        <label className="qb-check-row">
          <input
            type="checkbox"
            checked={enabled}
            // 存过的那一份读回来之前不让动：不然读回来的「开着」会盖掉刚点的。
            disabled={!loaded}
            onChange={(e) => setEnabled(e.target.checked)}
          />
          开着（站点卡表头那一格用它问余额）
        </label>
        {error && <p className="notice notice--danger">{error}</p>}
        {result && (
          <p className="notice" data-testid="usage-script-result">
            试跑结果：{quotaLine(result).text}
            {result.plan ? ` · ${result.plan}` : ""}
            {result.warning ? ` · ${result.warning}` : ""}
          </p>
        )}
      </div>
    </Modal>
  );
}

/**
 * IP 纯净度 —— 拆成几块，安全页排完整版，总览评分那一格用 `compact`。
 *
 * `compact` **只控排版，不控内容**：紧凑版少的是卡片外壳和指标网格，
 * 两家站点的入口和两个勾选按钮一个都不少 —— 那是这一项唯一能得分的路径。
 */
import { CheckCircle2, RotateCw, ShieldAlert, XCircle } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";

import { useProgress } from "../../lib/progress";
import { R } from "../../lib/resources";
import { useResource } from "../../lib/store";
import {
  Button,
  Card,
  Collapsible,
  ExternalLink,
  Metric,
  Pill,
  ProgressBar,
  Row,
  useToast,
  CHECK_LABEL,
  CHECK_TONE,
} from "../../ui";
import { judgePurity, useChecks } from "./useChecks";

/**
 * 人工复核的当前结论。`undefined` = 还没复核过。
 *
 * 旧代码把勾选记在 `useSession('purity.manual')` 里，于是重启面板之后
 * 「不合格」那张红卡片就不见了，而 `progress.json` 里那条还写着 failed ——
 * 同一件事两份状态，必然对不上。这里只认 progress：`judgePurity` 一落盘
 * 它就是新的，而且活得过重启。
 */
function useVerdict(): "passed" | "failed" | "skipped" | undefined {
  const rec = useProgress().steps["purity"];
  if (!rec || rec.state === "pending") return undefined;
  return rec.state;
}

/** 复核结论的 pill，给对象列表和详情标题用。 */
export function PurityPill() {
  const state = useVerdict();
  return (
    <Pill
      tone={
        state === "passed"
          ? "ok"
          : state === "failed"
            ? "danger"
            : state === "skipped"
              ? "warn"
              : "default"
      }
    >
      {state === "passed"
        ? "已通过"
        : state === "failed"
          ? "不合格"
          : state === "skipped"
            ? "已跳过"
            : "未复核"}
    </Pill>
  );
}

/**
 * 权威复核 —— 这一项**唯一**能得分的地方。
 *
 * 面板自测给不出结论（见下面 `PurityWhy`），所以这里必须把判定权明确交回去，
 * 而不是摆一个看起来像结论的数字。
 */
export function PurityVerdict({ compact = false }: { compact?: boolean }) {
  const toast = useToast();
  const ip = useResource("ip", R.ip);
  const crit = useResource("criteria", R.criteria);
  const state = useVerdict();

  async function choose(pass: boolean) {
    await judgePurity(pass, ip.data?.ip);
    if (pass) toast.ok("已记为通过");
    else toast.info("已记为不合格，总览会爆红");
  }

  const buttons = (
    <>
      <Button
        variant={state === "passed" ? "primary" : "default"}
        size={compact ? "sm" : undefined}
        icon={<CheckCircle2 size={13} />}
        onClick={() => void choose(true)}
      >
        两家均通过
      </Button>
      <Button
        variant={state === "failed" ? "danger" : "default"}
        size={compact ? "sm" : undefined}
        icon={<XCircle size={13} />}
        onClick={() => void choose(false)}
      >
        有不通过
      </Button>
    </>
  );

  if (compact) {
    return (
      <div>
        <p className="notice">
          <strong>面板不替你判定。</strong>
          自己打开这两个站点看结果，再回来勾选。
        </p>
        <div className="mt-2 flex flex-wrap gap-2">
          <Button
            size="sm"
            disabled={!crit.data}
            title={crit.data?.ipqs.criteria}
            onClick={() => crit.data && openUrl(crit.data.ipqs.url)}
          >
            IPQualityScore
          </Button>
          <Button
            size="sm"
            disabled={!crit.data}
            title={crit.data?.ippure.criteria}
            onClick={() => crit.data && openUrl(crit.data.ippure.url)}
          >
            ippure.com
          </Button>
          {buttons}
        </div>
      </div>
    );
  }

  return (
    <Card as="h3" title="权威复核" className="mb-3" tone="accent">
      <p className="notice mb-2">
        <strong>面板不替你判定。</strong>
        请自己打开这两个站点看结果，再回来勾选。下面的面板自测只影响提示，
        不决定通过与否。
      </p>

      <Row
        side={
          <Button
            variant="primary"
            size="sm"
            disabled={!crit.data}
            onClick={() => crit.data && openUrl(crit.data.ipqs.url)}
          >
            打开
          </Button>
        }
      >
        <span className="w-full">
          <span className="text-md">① IPQualityScore</span>
          <span className="notice block">
            {crit.data?.ipqs.criteria ?? "载入中…"}
          </span>
        </span>
      </Row>

      <Row
        side={
          <Button
            variant="primary"
            size="sm"
            disabled={!crit.data}
            onClick={() => crit.data && openUrl(crit.data.ippure.url)}
          >
            打开
          </Button>
        }
      >
        <span className="w-full">
          <span className="text-md">② ippure.com</span>
          <span className="notice block">
            {crit.data?.ippure.criteria ?? "载入中…"}
          </span>
        </span>
      </Row>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <span className="notice">查完在这里勾：</span>
        {buttons}
      </div>
    </Card>
  );
}

/** 勾了「有不通过」之后的代价说明。单独一块，好让它排在详情最上面。 */
export function PurityFailure() {
  const crit = useResource("criteria", R.criteria);
  const state = useVerdict();
  if (state !== "failed") return null;
  return (
    <Card tone="danger" className="mb-3">
      <div className="flex items-start gap-2">
        <ShieldAlert
          size={16}
          className="mt-0.5 flex-shrink-0"
          aria-hidden="true"
        />
        <div className="min-w-0">
          <div className="text-md text-[var(--danger)]">IP 不合格</div>
          <p className="notice notice--danger mt-1">
            继续用这条 IP 登录，风险由你自己承担。
          </p>
          <Button
            variant="danger"
            className="mt-2"
            disabled={!crit.data}
            onClick={() => crit.data && openUrl(crit.data.iproyal)}
          >
            前往 IPRoyal 购买住宅 IP
          </Button>
        </div>
      </div>
    </Card>
  );
}

/** 面板自测。标着「不权威」，因为它结构上就给不出「整体通过」。 */
export function PurityProbe() {
  const ip = useResource("ip", R.ip);
  const verdict = useResource("purity", R.purity);
  const crit = useResource("criteria", R.criteria);
  const check = useChecks().purity;

  const max = crit.data?.maxFraudScore ?? 5;
  const score = ip.data?.fraudScore;

  return (
    <Card
      as="h3"
      title={
        <>
          面板自测 <Pill tone="warn">不权威</Pill>
        </>
      }
      actions={
        <Button
          size="sm"
          icon={<RotateCw size={12} />}
          loading={check.running}
          onClick={() => void check.run()}
        >
          {check.done ? "重新检测" : "开始检测"}
        </Button>
      }
    >
      {check.error && (
        <p className="notice notice--danger mb-2">{check.error}</p>
      )}

      <div className="grid gap-2 sm:grid-cols-3">
        <Metric
          label="IPPure 系数"
          loading={ip.loading && !ip.data}
          error={ip.error}
          onRetry={() => void check.run()}
          emptyHint="接口没返回这个字段"
        >
          {score !== null && score !== undefined ? (
            <>
              {score}
              {verdict.data && (
                <Pill tone={CHECK_TONE[verdict.data.purity]}>
                  {CHECK_LABEL[verdict.data.purity]}
                </Pill>
              )}
            </>
          ) : undefined}
        </Metric>

        <Metric
          label="IP 属性"
          loading={ip.loading && !ip.data}
          error={ip.error}
          onRetry={() => void check.run()}
          emptyHint="接口没返回住宅 / 非住宅的判定"
        >
          {ip.data?.isResidential === true ? (
            <>
              住宅 IP
              {verdict.data && (
                <Pill tone={CHECK_TONE[verdict.data.residential]}>
                  {CHECK_LABEL[verdict.data.residential]}
                </Pill>
              )}
            </>
          ) : ip.data?.isResidential === false ? (
            <>
              非住宅
              {verdict.data && (
                <Pill tone={CHECK_TONE[verdict.data.residential]}>
                  {CHECK_LABEL[verdict.data.residential]}
                </Pill>
              )}
            </>
          ) : undefined}
        </Metric>

        <Metric
          label="IP 来源"
          loading={verdict.loading && !verdict.data}
          error={verdict.error}
          onRetry={() => void check.run()}
          emptyText="未知"
          emptyHint="「原生 IP」公开接口就是没有这个字段，只能去站点上看"
        >
          {verdict.data && verdict.data.native !== "Unknown" ? (
            <>
              原生 IP
              <Pill tone={CHECK_TONE[verdict.data.native]}>
                {CHECK_LABEL[verdict.data.native]}
              </Pill>
            </>
          ) : undefined}
        </Metric>
      </div>

      {score !== null && score !== undefined && (
        <ProgressBar
          className="mt-2"
          value={Math.min(100, score)}
          tone={score <= max ? "ok" : "danger"}
          label={`IPPure 系数 ${score}，阈值 ${max}`}
        />
      )}

      <div className="mt-3 grid gap-2 sm:grid-cols-3">
        <Metric
          label="出口 IP"
          mono
          loading={ip.loading && !ip.data}
          error={ip.error}
        >
          {ip.data?.ip}
        </Metric>
        <Metric
          label="ASN / 运营商"
          loading={ip.loading && !ip.data}
          error={ip.error}
        >
          {ip.data?.asOrganization}
        </Metric>
        <Metric
          label="位置 / 时区"
          loading={ip.loading && !ip.data}
          error={ip.error}
        >
          {ip.data
            ? `${ip.data.countryCode ?? "—"} · ${ip.data.timezone ?? "—"}`
            : undefined}
        </Metric>
      </div>

      <p className="notice mt-3">{verdict.data?.note}</p>

      {!!crit.data?.optional.length && (
        <div className="mt-2 flex flex-wrap gap-2">
          {crit.data.optional.map((o) => (
            <ExternalLink key={o.name} href={o.url} asButton>
              {o.name}
            </ExternalLink>
          ))}
        </div>
      )}
    </Card>
  );
}

export function PurityWhy() {
  return (
    <Collapsible className="mt-3" summary="为什么面板不直接给结论？">
      <p className="notice">
        面板自测走的是 <code>my.ippure.com/v1/info</code> 这个公开接口，它返回
        <code>fraudScore</code>、<code>isResidential</code>、
        <code>timezone</code>、<code>asn</code>，但
        <strong>没有「原生 IP」这一项</strong>。 三项硬指标缺一不可，
        缺了一项就永远给不出「整体通过」—— 与其猜一个，不如如实报「未知」，
        把判定权交回给你。
      </p>
      <p className="notice mt-2">
        拿不到的字段一律报未知，不猜成通过。这条是刻意的：一个会猜的检测
        比没有检测更危险，因为它会让人以为验过了。
      </p>
    </Collapsible>
  );
}

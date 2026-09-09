import { CheckCircle2, RotateCw, ShieldAlert, SkipForward, XCircle } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';

import { useNav } from '../lib/nav';
import { R } from '../lib/resources';
import { useResource, useSession } from '../lib/store';
import {
  Button,
  Card,
  Collapsible,
  ExternalLink,
  Metric,
  PageHeader,
  Pill,
  ProgressBar,
  Row,
  useToast,
  CHECK_LABEL,
  CHECK_TONE,
} from '../ui';

type Manual = 'unset' | 'pass' | 'fail';

export default function Purity() {
  const { mark, progress } = useNav();
  const toast = useToast();

  const ip = useResource('ip', R.ip);
  const verdict = useResource('purity', R.purity);
  const crit = useResource('criteria', R.criteria);

  // 勾选活在 store 里 —— 旧代码是纯 useState，去 DNS 页看一眼回来就得重勾。
  const [manual, setManual] = useSession<Manual>('purity.manual', 'unset');

  const rec = progress.steps['purity'];
  const max = crit.data?.maxFraudScore ?? 5;
  const score = ip.data?.fraudScore;

  async function choose(v: Manual) {
    setManual(v);
    if (v === 'pass') {
      await mark('purity', 'passed', 'low', `已人工确认两家均通过（${ip.data?.ip ?? '未知 IP'}）`);
      toast.ok('已记为通过');
    } else if (v === 'fail') {
      await mark('purity', 'failed', 'high', '人工复核未通过：三项硬指标至少缺一');
      toast.info('已记为不合格，总览会爆红');
    }
  }

  async function skip() {
    setManual('unset');
    await mark('purity', 'skipped', 'unknown', '用户强制跳过，未做人工复核');
    toast.info('已标记为跳过');
  }

  return (
    <>
      <PageHeader
        title="IP 纯净度"
        sub={`三项硬指标缺一不可：纯净度 ≤ ${max}%、原生 IP、住宅 IP。`}
        actions={
          rec && (
            <Pill
              tone={
                rec.state === 'passed'
                  ? 'ok'
                  : rec.state === 'failed'
                    ? 'danger'
                    : rec.state === 'skipped'
                      ? 'warn'
                      : 'default'
              }
            >
              {rec.state === 'passed'
                ? '已通过'
                : rec.state === 'failed'
                  ? '不合格'
                  : rec.state === 'skipped'
                    ? '已跳过'
                    : '未复核'}
            </Pill>
          )
        }
      />

      {manual === 'fail' && (
        <Card tone="danger" className="mb-3">
          <div className="flex items-start gap-2">
            <ShieldAlert size={16} className="mt-0.5 flex-shrink-0" aria-hidden="true" />
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
      )}

      {/* -------------------------------------------------- 权威复核 */}
      <Card title="权威复核" className="mb-3" tone="accent">
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
            <span className="notice block">{crit.data?.ipqs.criteria ?? '载入中…'}</span>
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
            <span className="notice block">{crit.data?.ippure.criteria ?? '载入中…'}</span>
          </span>
        </Row>

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <span className="notice">查完在这里勾：</span>
          <Button
            variant={manual === 'pass' ? 'primary' : 'default'}
            icon={<CheckCircle2 size={13} />}
            onClick={() => choose('pass')}
          >
            两家均通过
          </Button>
          <Button
            variant={manual === 'fail' ? 'danger' : 'default'}
            icon={<XCircle size={13} />}
            onClick={() => choose('fail')}
          >
            有不通过
          </Button>
        </div>
      </Card>

      {/* -------------------------------------------------- 面板自测 */}
      <Card
        title={
          <>
            面板自测 <Pill tone="warn">不权威</Pill>
          </>
        }
        actions={
          <Button
            size="sm"
            icon={<RotateCw size={12} />}
            loading={ip.loading || verdict.loading}
            onClick={() => {
              void ip.refresh();
              void verdict.refresh();
            }}
          >
            重新检测
          </Button>
        }
      >
        <div className="grid gap-2 sm:grid-cols-3">
          <Metric
            label="IPPure 系数"
            loading={ip.loading && !ip.data}
            error={ip.error}
            onRetry={() => void ip.refresh()}
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
            onRetry={() => void ip.refresh()}
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
            onRetry={() => void verdict.refresh()}
            emptyText="未知"
            emptyHint="「原生 IP」公开接口就是没有这个字段，只能去站点上看"
          >
            {verdict.data && verdict.data.native !== 'Unknown' ? (
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
            tone={score <= max ? 'ok' : 'danger'}
            label={`IPPure 系数 ${score}，阈值 ${max}`}
          />
        )}

        <div className="mt-3 grid gap-2 sm:grid-cols-3">
          <Metric label="出口 IP" mono loading={ip.loading && !ip.data} error={ip.error}>
            {ip.data?.ip}
          </Metric>
          <Metric label="ASN / 运营商" loading={ip.loading && !ip.data} error={ip.error}>
            {ip.data?.asOrganization}
          </Metric>
          <Metric label="位置 / 时区" loading={ip.loading && !ip.data} error={ip.error}>
            {ip.data ? `${ip.data.countryCode ?? '—'} · ${ip.data.timezone ?? '—'}` : undefined}
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

      <Collapsible className="mt-3" summary="为什么面板不直接给结论？">
        <p className="notice">
          面板自测走的是 <code>my.ippure.com/v1/info</code> 这个公开接口，它返回
          <code>fraudScore</code>、<code>isResidential</code>、<code>timezone</code>、
          <code>asn</code>，但<strong>没有「原生 IP」这一项</strong>。
          三项硬指标缺一不可，缺了一项就永远给不出「整体通过」——
          与其猜一个，不如如实报「未知」，把判定权交回给你。
        </p>
        <p className="notice mt-2">
          拿不到的字段一律报未知，不猜成通过。这条是刻意的：一个会猜的检测
          比没有检测更危险，因为它会让人以为验过了。
        </p>
      </Collapsible>

      <div className="mt-4 flex justify-end">
        <Button variant="ghost" icon={<SkipForward size={13} />} onClick={skip}>
          标记为已跳过
        </Button>
      </div>
    </>
  );
}

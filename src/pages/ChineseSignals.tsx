import { Play, RotateCw, Wrench } from 'lucide-react';

import { R } from '../lib/resources';
import { useResource } from '../lib/store';
import { useNav } from '../lib/nav';
import {
  Button,
  Card,
  Collapsible,
  EmptyState,
  ExternalLink,
  Metric,
  PageHeader,
  Pill,
  ProgressBar,
  Row,
} from '../ui';

const BAND_LABEL = { low: '低', medium: '中', high: '高' } as const;
const BAND_TONE = { low: 'ok', medium: 'warn', high: 'danger' } as const;

/**
 * 中文环境识别。
 *
 * 从旧 `Environment.tsx` 里拆出来 —— 那个文件 391 行塞了五件事
 * （软件检测 / 安装 / 升级 / 时区 / 中文环境识别），而这一项本身是完整独立的
 * 话题：10 项加权指纹，背后 475 行 `signals.ts`。
 *
 * 它**没有自己的进度步骤**，归在 `environment` 那一步里，由环境页负责记录 ——
 * `progress.json` 的五个 key 一个都不能多，见 `steps.ts`。
 */
export default function ChineseSignals() {
  const { go } = useNav();
  const scan = useResource('signals', R.signals);
  const r = scan.data;

  return (
    <>
      <PageHeader
        title="中文环境识别"
        sub="十项加权指纹，满分 100，全部在本地算，不上传任何数据。"
        actions={
          <Button
            variant={r ? 'default' : 'primary'}
            icon={r ? <RotateCw size={13} /> : <Play size={13} />}
            loading={scan.loading}
            onClick={() => void scan.refresh()}
          >
            {scan.loading ? '检测中…' : r ? '重新检测' : '开始检测'}
          </Button>
        }
      />

      {scan.error && <p className="notice notice--danger mb-3">{scan.error}</p>}

      {!r && !scan.loading ? (
        <Card>
          <EmptyState
            title="还没检测过"
            action={
              <Button variant="primary" icon={<Play size={13} />} onClick={() => void scan.refresh()}>
                开始检测
              </Button>
            }
          >
            检测浏览器语言、已装字体、WebRTC、Emoji 渲染这些浏览器能读到的信号，
            以及系统时区。其中 WebRTC 那一项有 1 秒超时，整体大约两三秒。
          </EmptyState>
        </Card>
      ) : (
        <>
          <Card className="mb-3">
            <div className="flex items-end gap-3">
              <span className="text-[32px] leading-none font-medium">
                {scan.loading && !r ? '…' : (r?.total ?? '—')}
              </span>
              <span className="notice mb-1">/ 100</span>
              {r && (
                <span className="mb-1">
                  <Pill tone={BAND_TONE[r.band]}>{BAND_LABEL[r.band]}风险</Pill>
                </span>
              )}
              {r && (
                <span className="notice mb-1 ml-auto">命中 {r.hits.length} 项</span>
              )}
            </div>
            {r && (
              <ProgressBar
                className="mt-2"
                value={r.total}
                tone={BAND_TONE[r.band]}
                label={`中文环境识别得分 ${r.total} / 100`}
              />
            )}
            <p className="notice mt-2">
              分档：低 0–30、中 31–60、高 61–100；单项 score ≥ 0.25 计为命中。
            </p>
          </Card>

          {r && (
            <Card title="逐项" className="mb-3">
              {r.signals.map((s) => (
                <Row
                  key={s.id}
                  side={
                    <>
                      <span className="notice max-w-[16ch] truncate" title={s.raw}>
                        {s.raw}
                      </span>
                      <Pill tone={BAND_TONE[s.verdict]}>+{s.points}</Pill>
                    </>
                  }
                >
                  <span title={s.hint}>{s.label}</span>
                  <span className="notice">权重 {s.weight}</span>
                  {s.claudeUsed && <Pill tone="accent">Claude 会读</Pill>}
                  {!s.fixable && s.points > 0 && <Pill tone="warn">不可修复</Pill>}
                </Row>
              ))}
              {/* 逐项的 points 是各自四舍五入的，加起来不等于上面那个总分
                  （总分是先求和再取整）。所以这里不给「小计」，免得对不上。 */}
              <p className="notice mt-2">
                逐项分数各自取整，相加与上方总分可能差 1–2 分 ——
                总分是先求和再取整的，以总分为准。
              </p>
            </Card>
          )}

          {r && (
            <Card title="能修的与修不了的" icon={<Wrench size={14} />} className="mb-3">
              <div className="grid gap-2 sm:grid-cols-2">
                <Metric label="能修" hint="改系统设置或卸载对应软件就能降下来">
                  {r.signals.filter((s) => s.fixable && s.points > 0).length} 项 ·{' '}
                  {r.signals.filter((s) => s.fixable).reduce((a, s) => a + s.points, 0)} 分
                </Metric>
                <Metric label="修不了" hint="删掉会让中文显示整体崩坏，代价远大于收益">
                  {r.signals.filter((s) => !s.fixable && s.points > 0).length} 项 ·{' '}
                  {r.signals.filter((s) => !s.fixable).reduce((a, s) => a + s.points, 0)} 分
                </Metric>
              </div>
              <p className="notice mt-3">
                <strong>已装中文字体那 14 分修不掉</strong>：删掉系统中文字体会让中文
                显示整体崩坏，代价远大于收益。面板如实展示分数并标注「不可修复」，
                <strong>不提供修复按钮，也不假装能修</strong>。
              </p>
              <div className="mt-2 flex flex-wrap gap-2">
                <Button onClick={() => go('environment')}>去对齐系统时区</Button>
              </div>
            </Card>
          )}
        </>
      )}

      <Collapsible summary="两条不能改错的判定">
        <p className="notice">
          <strong>台湾不计分。</strong>
          <code>Asia/Taipei</code> 与 <code>zh-TW</code> 是 Anthropic 完整支持的地区，
          给它们加分属于误报。港澳属受限地区，保留部分风险分。
        </p>
        <p className="notice mt-2">
          <strong>繁体字体压在命中阈值以下。</strong>
          繁体字体在台湾很常见，而且区分不出 TW 与 HK/MO，所以给的分刻意低于
          0.25 的命中线。
        </p>
        <p className="notice mt-2">
          这两条原样继承自上游 issue，动了就是误报。
        </p>
      </Collapsible>

      <Collapsible summary="来源与那个「未证实」的说法">
        <p className="notice">
          评分逻辑改编自{' '}
          <ExternalLink href="https://github.com/LinXiaoTao/FuckClaude">FuckClaude</ExternalLink>
          （MIT），逐条说明见仓库根目录的 ATTRIBUTION.md。
        </p>
        <p className="notice mt-2">
          另有一种说法称 Claude Code 会把系统时区用 Unicode 隐写术藏进 system
          prompt。这是<strong>第三方逆向分析主张，本项目未做独立验证</strong>，
          不作为既定事实。注意其描述的触发条件是走中转端点，
          官方 OAuth 直连不在该描述范围内。
        </p>
      </Collapsible>
    </>
  );
}

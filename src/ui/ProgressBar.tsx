import type { Tone } from './labels';

interface Props {
  /** 0–100。不给就是 indeterminate（只知道在跑，不知道还要多久）。 */
  value?: number;
  tone?: Exclude<Tone, 'accent' | 'default'> | 'accent';
  label?: string;
  className?: string;
}

const FILL: Record<string, string> = {
  accent: '',
  ok: 'bar-fill--ok',
  warn: 'bar-fill--warn',
  danger: 'bar-fill--danger',
};

export default function ProgressBar({ value, tone = 'accent', label, className = '' }: Props) {
  const indeterminate = value === undefined;
  const pct = indeterminate ? 0 : Math.max(0, Math.min(100, value));

  return (
    <div
      className={`bar ${className}`}
      role="progressbar"
      aria-label={label}
      aria-valuenow={indeterminate ? undefined : Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <span
        className={`bar-fill ${FILL[tone] ?? ''}${indeterminate ? ' bar-fill--indeterminate' : ''}`}
        style={indeterminate ? undefined : { width: `${pct}%` }}
      />
    </div>
  );
}

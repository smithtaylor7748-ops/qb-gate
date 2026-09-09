import { AlertCircle, RotateCw } from 'lucide-react';
import type { ReactNode } from 'react';
import Skeleton from './Skeleton';

interface Props {
  label: ReactNode;
  /** 数值。`undefined` 表示确实没有这项数据，会显示带说明的占位。 */
  children?: ReactNode;
  /** 正在拉 —— 显示骨架屏，**不要**显示「—」。 */
  loading?: boolean;
  /** 拉失败 —— 显示红字与重试，**也不要**显示「—」。 */
  error?: string;
  onRetry?: () => void;
  /** 空值时的解释。旧代码一个光秃秃的「—」，用户分不清是没查到还是没这项。 */
  emptyHint?: string;
  emptyText?: string;
  mono?: boolean;
  /** 标签旁的问号提示。 */
  hint?: string;
  className?: string;
}

/**
 * 三态内建的指标块。
 *
 * 这是整个改版里最要紧的一个组件。旧 Dashboard 六个请求全写成
 * `.catch(() => undefined)`，于是**加载中、请求失败、确实没有这项数据**
 * 三种完全不同的情况在界面上都长成同一个「—」——
 * 用户没法知道是网断了还是本来就没有。
 */
export default function Metric({
  label,
  children,
  loading = false,
  error,
  onRetry,
  emptyHint = '没有拿到这项数据',
  emptyText = '—',
  mono = false,
  hint,
  className = '',
}: Props) {
  const isEmpty = children === undefined || children === null || children === '';

  return (
    <div className={`metric ${className}`}>
      <span className="metric-k" title={hint}>
        {label}
      </span>

      {loading ? (
        <Skeleton className="h-[18px] w-3/5" />
      ) : error ? (
        <span className="metric-error">
          <AlertCircle size={12} aria-hidden="true" />
          <span className="min-w-0 truncate" title={error}>
            读取失败
          </span>
          {onRetry && (
            <button
              type="button"
              className="btn btn--ghost btn--sm !p-1"
              onClick={onRetry}
              aria-label={`重试读取${typeof label === 'string' ? label : ''}`}
            >
              <RotateCw size={11} aria-hidden="true" />
            </button>
          )}
        </span>
      ) : (
        <span className={`metric-v${mono ? ' mono' : ''}`}>
          {isEmpty ? (
            <span className="metric-empty" title={emptyHint}>
              {emptyText}
            </span>
          ) : (
            children
          )}
        </span>
      )}
    </div>
  );
}

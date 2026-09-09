import type { ButtonHTMLAttributes, ReactNode } from 'react';

export type ButtonVariant = 'default' | 'primary' | 'danger' | 'ghost';

interface Props extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'className'> {
  variant?: ButtonVariant;
  size?: 'sm' | 'md';
  /**
   * 只让**这一个**按钮转圈并禁用。
   *
   * 旧代码是一个 `busy` 布尔量禁用整页七个按钮 —— 点「检查版本」，
   * 连「切换时区」都变灰了，看起来像整个面板卡死。
   */
  loading?: boolean;
  icon?: ReactNode;
  /** 大号按钮，只用在总览的启动区。 */
  launch?: boolean;
  block?: boolean;
  children?: ReactNode;
  className?: string;
}

export default function Button({
  variant = 'default',
  size = 'md',
  loading = false,
  icon,
  launch = false,
  block = false,
  disabled,
  children,
  className = '',
  type = 'button',
  ...rest
}: Props) {
  const cls = [
    'btn',
    `btn--${size}`,
    variant !== 'default' && `btn--${variant}`,
    launch && 'btn--launch',
    block && 'w-full',
    className,
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <button
      {...rest}
      type={type}
      className={cls}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
    >
      {loading ? <span className="btn-spinner" aria-hidden="true" /> : icon}
      {children}
    </button>
  );
}

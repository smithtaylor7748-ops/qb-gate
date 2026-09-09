import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { AlertTriangle } from 'lucide-react';
import Button from './Button';

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  icon?: ReactNode;
  danger?: boolean;
  children?: ReactNode;
  footer?: ReactNode;
  /** 关不掉的弹窗（正在执行破坏性操作时）。Esc 与点背景都不生效。 */
  dismissible?: boolean;
}

/**
 * 用原生 `<dialog>` + `showModal()`。
 *
 * 焦点陷阱、Esc 关闭、背景 inert 全都是浏览器自带的，不需要自己实现，
 * 也就不需要 Radix。旧代码一个弹窗都没有 —— 应急解锁、清理残留、删白名单、
 * 恢复备份、切换账户、关进程，**全部零二次确认**。
 */
export function Modal({
  open,
  onClose,
  title,
  icon,
  danger = false,
  children,
  footer,
  dismissible = true,
}: ModalProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    else if (!open && d.open) d.close();
  }, [open]);

  if (!open) return null;

  return (
    <dialog
      ref={ref}
      className="modal"
      aria-labelledby={titleId}
      onKeyDown={(e) => {
        // Esc 自己处理，**不依赖原生的 cancel 事件**。
        //
        // 原生 `<dialog>` 的 Esc 关闭走的是浏览器内部的 close watcher，
        // 而它在这个环境里实测不触发：keydown 收得到 Escape，cancel 却一次都没发。
        // 弹窗关不掉是个很难受的失败 —— 破坏性操作的确认框把人困住 ——
        // 所以这里直接接管，preventDefault 顺带避免原生行为再触发一次。
        if (e.key !== 'Escape') return;
        e.preventDefault();
        e.stopPropagation();
        if (dismissible) onClose();
      }}
      onCancel={(e) => {
        // 原生 cancel 真的来了也接住，让 React 的 open 状态跟得上 ——
        // 否则 DOM 关了而 React 还以为开着，下次就打不开了。
        e.preventDefault();
        if (dismissible) onClose();
      }}
      onClick={(e) => {
        // 点在 backdrop 上（事件目标就是 dialog 本身）才关。
        if (dismissible && e.target === ref.current) onClose();
      }}
    >
      <div className="modal-head">
        <h2 id={titleId} className={`modal-title${danger ? ' modal-title--danger' : ''}`}>
          {icon ?? (danger ? <AlertTriangle size={16} aria-hidden="true" /> : null)}
          {title}
        </h2>
      </div>
      <div className="modal-body">{children}</div>
      {footer && <div className="modal-foot">{footer}</div>}
    </dialog>
  );
}

interface ConfirmProps {
  open: boolean;
  onCancel: () => void;
  onConfirm: () => void;
  title: ReactNode;
  /** 后果说明。**说清楚会发生什么**，不要只写「确定吗？」。 */
  children: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  loading?: boolean;
  /**
   * 要求用户手打这个词才能确认。
   *
   * 只给最贵的那几个操作用：恢复备份会覆盖世界书和角色卡，
   * 应急解锁会把全部执行锁摘掉。
   */
  confirmWord?: string;
}

export function ConfirmDialog({
  open,
  onCancel,
  onConfirm,
  title,
  children,
  confirmLabel = '确认',
  cancelLabel = '取消',
  danger = false,
  loading = false,
  confirmWord,
}: ConfirmProps) {
  const [typed, setTyped] = useState('');
  const inputId = useId();

  // 每次打开都清空，避免上一次输过的确认词还留着。
  useEffect(() => {
    if (open) setTyped('');
  }, [open]);

  const ready = !confirmWord || typed.trim() === confirmWord;

  return (
    <Modal
      open={open}
      onClose={loading ? () => undefined : onCancel}
      title={title}
      danger={danger}
      dismissible={!loading}
      footer={
        <>
          <Button onClick={onCancel} disabled={loading}>
            {cancelLabel}
          </Button>
          <Button
            variant={danger ? 'danger' : 'primary'}
            onClick={onConfirm}
            disabled={!ready}
            loading={loading}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      {children}

      {confirmWord && (
        <div className="mt-3">
          <label className="field-label" htmlFor={inputId}>
            确认无误请输入 <code>{confirmWord}</code>
          </label>
          <input
            id={inputId}
            className="input"
            type="text"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            autoComplete="off"
            spellCheck={false}
            disabled={loading}
          />
        </div>
      )}
    </Modal>
  );
}

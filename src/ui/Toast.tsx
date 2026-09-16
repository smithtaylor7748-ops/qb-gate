import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { CheckCircle2, Info, X, XCircle } from "lucide-react";

type ToastKind = "ok" | "danger" | "info";

interface Toast {
  id: number;
  kind: ToastKind;
  text: string;
}

interface ToastApi {
  ok: (text: string) => void;
  error: (text: string) => void;
  info: (text: string) => void;
  /** 把一个可能抛错的操作包起来：成功报成功，失败把错误原文弹出来。 */
  run: <T>(fn: () => Promise<T>, okText?: string) => Promise<T | undefined>;
}

const Ctx = createContext<ToastApi | null>(null);

/** 成功 3 秒自己消失；错误常驻，得让人有时间看完再关。 */
const OK_MS = 3000;

const ICON: Record<ToastKind, ReactNode> = {
  ok: <CheckCircle2 size={14} aria-hidden="true" />,
  danger: <XCircle size={14} aria-hidden="true" />,
  info: <Info size={14} aria-hidden="true" />,
};

/**
 * 右下角的操作反馈。
 *
 * 旧代码的成功提示是页面**顶部**一行 11px 灰字，而按钮往往在页面底部 ——
 * 点完「启动 Claude Code」，用户眼前什么都不变，反馈在看不见的地方。
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [list, setList] = useState<Toast[]>([]);
  const seq = useRef(0);

  const remove = useCallback((id: number) => {
    setList((l) => l.filter((t) => t.id !== id));
  }, []);

  const push = useCallback(
    (kind: ToastKind, text: string) => {
      const id = ++seq.current;
      setList((l) => [...l, { id, kind, text }]);
      if (kind === "ok" || kind === "info") {
        setTimeout(() => remove(id), OK_MS);
      }
    },
    [remove],
  );

  const api = useMemo<ToastApi>(
    () => ({
      ok: (t) => push("ok", t),
      error: (t) => push("danger", t),
      info: (t) => push("info", t),
      run: async (fn, okText) => {
        try {
          const r = await fn();
          if (okText) push("ok", okText);
          return r;
        } catch (e) {
          push("danger", e instanceof Error ? e.message : String(e));
          return undefined;
        }
      },
    }),
    [push],
  );

  return (
    <Ctx.Provider value={api}>
      {children}
      {/* 成功用 polite、错误用 assertive 会更细，但混在一个区域里读屏器行为
          反而乱；统一 polite，错误另有常驻视觉。 */}
      <div className="toast-region" role="status" aria-live="polite">
        {list.map((t) => (
          <div key={t.id} className={`toast toast--${t.kind}`}>
            <span className="toast-icon">{ICON[t.kind]}</span>
            <span className="toast-text">{t.text}</span>
            <button
              type="button"
              className="toast-close"
              onClick={() => remove(t.id)}
              aria-label="关闭这条提示"
            >
              <X size={13} aria-hidden="true" />
            </button>
          </div>
        ))}
      </div>
    </Ctx.Provider>
  );
}

export function useToast(): ToastApi {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useToast 必须在 <ToastProvider> 里用");
  return ctx;
}

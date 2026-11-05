import { useCallback, useEffect, useState } from "react";
import { Archive } from "lucide-react";
import type { ArchivedSlot } from "../../lib/generated/ArchivedSlot";
import { Button, Modal, useToast } from "../../ui";

/**
 * 「已归档的账户」（GPT / 反重力共用）。页面上只占一个小按钮，点开是弹窗 ——
 * 账户页 900×740 的布局有断言钉着，折叠区放不下。
 *
 * 「恢复」= 后端把归档目录改名挪回列表（绝不复制，免得本机多一份令牌）。
 */
export default function ArchivedSlots({
  load,
  restore,
  onRestored,
  version,
}: {
  load: () => Promise<ArchivedSlot[]>;
  restore: (key: string) => Promise<void>;
  onRestored: () => void;
  /** 变一下就重读（移除完、恢复完）。 */
  version: number;
}) {
  const toast = useToast();
  const [items, setItems] = useState<ArchivedSlot[]>([]);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const reload = useCallback(() => {
    load()
      .then(setItems)
      .catch(() => setItems([]));
  }, [load]);
  useEffect(() => reload(), [reload, version]);
  if (items.length === 0) return null;
  async function back(key: string) {
    setBusy(key);
    setError("");
    try {
      await restore(key);
      toast.ok("已放回列表");
      reload();
      onRestored();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  }
  return (
    <>
      <Button
        size="sm"
        variant="ghost"
        icon={<Archive size={12} />}
        onClick={() => setOpen(true)}
      >
        已归档 {items.length}
      </Button>
      <Modal
        open={open}
        onClose={() => !busy && setOpen(false)}
        title="已归档的账户"
      >
        <p className="notice">
          移除的账户目录留在本机归档里。「恢复」把它原样挪回列表，不复制任何文件。
        </p>
        <ul className="mt-2 grid gap-1.5">
          {items.map((a) => (
            <li key={a.key} className="flex items-center gap-2">
              <span className="truncate">{a.label}</span>
              <span className="notice">
                {a.pending
                  ? "目录还没挪进归档（当时被占着，稍后再试）"
                  : a.archived_at
                    ? `归档于 ${new Date(a.archived_at).toLocaleString()}`
                    : "旧版本归档"}
              </span>
              <Button
                size="sm"
                className="ml-auto"
                loading={busy === a.key}
                disabled={!!busy}
                onClick={() => void back(a.key)}
              >
                恢复
              </Button>
            </li>
          ))}
        </ul>
        {error && (
          <p role="alert" className="notice notice--danger mt-2">
            {error}
          </p>
        )}
      </Modal>
    </>
  );
}

/** 后端「目录被占着、一个文件都没动」那类错误的开头（`archive_dir::BUSY_PREFIX`）。 */
export const ARCHIVE_BUSY_PREFIX = "没动任何文件；";

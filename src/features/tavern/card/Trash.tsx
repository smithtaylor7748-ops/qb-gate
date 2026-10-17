/**
 * 酒馆的回收站（2026-09-29）：角色卡、世界书、扩展「删除」时都先挪到这里（`state_dir\tavern-trash`）。
 *
 * - 「放回去」= 原样挪回原来的位置；原处已经有同名的就不动（不覆盖），照实说；
 * - 「删掉」/「清空回收站」才是真删，走确认框、写清楚删几项、多大。
 */
import { useState } from "react";
import { Trash, Undo2 } from "lucide-react";

import { api, type TrashEntry } from "../../../lib/api";
import { AFTER, R } from "../../../lib/resources";
import { invalidate, useResource } from "../../../lib/store";
import {
  Button,
  ConfirmDialog,
  EmptyState,
  Pill,
  Row,
  fmtSize,
  useToast,
} from "../../../ui";
import { errorText } from "./common";

const KIND: Record<string, string> = {
  character: "角色卡",
  world: "世界书",
  extension: "扩展",
};

export default function TrashList() {
  const toast = useToast();
  const trash = useResource("tavernTrash", R.tavernTrash);
  const [busy, setBusy] = useState("");
  const [purging, setPurging] = useState<TrashEntry | "all" | null>(null);
  const items = trash.data ?? [];
  const total = items.reduce((n, e) => n + e.size, 0);

  async function restore(e: TrashEntry) {
    setBusy("restore:" + e.id);
    try {
      await api.tavernTrashRestore(e.id);
      toast.ok(`「${e.name}」放回去了`);
      invalidate(...AFTER.tavernLibrary);
    } catch (err) {
      toast.error(errorText(err));
    } finally {
      setBusy("");
    }
  }

  async function purge() {
    if (!purging) return;
    setBusy("purge");
    try {
      const n = await api.tavernTrashPurge(
        purging === "all" ? null : purging.id,
      );
      toast.ok(`回收站里删掉了 ${n} 项`);
      invalidate(...AFTER.tavernLibrary);
      setPurging(null);
    } catch (err) {
      toast.error(errorText(err));
    } finally {
      setBusy("");
    }
  }

  return (
    <section
      className="tv-block"
      aria-labelledby="tv-trash"
      data-testid="tavern-trash"
    >
      <div className="tv-block-head">
        <h3 id="tv-trash" className="tv-block-title">
          <Trash size={13} aria-hidden="true" /> 回收站
        </h3>
        <Button
          size="sm"
          variant="danger"
          disabled={!items.length || !!busy}
          onClick={() => setPurging("all")}
        >
          清空回收站
        </Button>
      </div>
      <p className="notice">
        在面板里「挪进回收站」的角色卡、世界书、扩展都在这里，能原样放回去。
        {items.length ? ` 现在 ${items.length} 项，共 ${fmtSize(total)}。` : ""}
      </p>
      {trash.error ? (
        <p className="notice notice--danger mt-2">{trash.error}</p>
      ) : items.length === 0 ? (
        <EmptyState title="回收站是空的">挪进来的东西会出现在这里。</EmptyState>
      ) : (
        items.map((e) => (
          <Row
            key={e.id}
            side={
              <span className="flex flex-wrap gap-1.5">
                <Button
                  size="sm"
                  icon={<Undo2 size={12} />}
                  loading={busy === "restore:" + e.id}
                  disabled={!!busy}
                  onClick={() => void restore(e)}
                >
                  放回去
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={!!busy}
                  onClick={() => setPurging(e)}
                >
                  删掉
                </Button>
              </span>
            }
          >
            <span className="min-w-0">
              <span className="flex flex-wrap items-center gap-1.5">
                <Pill>{KIND[e.kind] ?? e.kind}</Pill>
                <strong className="break-all">{e.name}</strong>
              </span>
              <span className="notice block break-all">
                {e.deleted_at} · {fmtSize(e.size)} · 原来在{" "}
                {e.originals.join("、") || "?"}
              </span>
            </span>
          </Row>
        ))
      )}
      <ConfirmDialog
        open={purging !== null}
        onCancel={() => setPurging(null)}
        onConfirm={() => void purge()}
        title={
          purging === "all"
            ? `清空回收站（${items.length} 项）？`
            : `真删「${purging?.name ?? ""}」？`
        }
        confirmLabel={purging === "all" ? "全部删掉" : "删掉"}
        loading={busy === "purge"}
        danger
      >
        <p>
          <strong>不可逆。</strong>
          {purging === "all"
            ? `回收站里的 ${items.length} 项（${fmtSize(total)}）从磁盘上删掉，放不回去了。`
            : "这一项从磁盘上删掉，放不回去了。"}
        </p>
      </ConfirmDialog>
    </section>
  );
}

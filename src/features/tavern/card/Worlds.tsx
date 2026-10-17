/**
 * 「酒馆」弹窗的「世界书」分页（2026-09-29；第三轮从软件页的卡搬进弹窗）。
 *
 * 列出 `data\default-user\worlds\` 里的世界书（条目数、启用了几条），点开看条目（可搜）；导入酒馆自己格式的 JSON；
 * 挪进回收站。**改条目在酒馆里做。** 别家格式（NovelAI / Agnai……）在酒馆里导入，它会转换 —— 面板不替它转，
 * 导入时如实说为什么没进来。
 */
import { useEffect, useState } from "react";
import { Eye, RefreshCw, Trash2, Upload } from "lucide-react";

import {
  api,
  type ImportReport,
  type WorldDetail,
  type WorldInfo,
} from "../../../lib/api";
import { AFTER, R } from "../../../lib/resources";
import { invalidate, useResource } from "../../../lib/store";
import {
  Button,
  ConfirmDialog,
  EmptyState,
  Modal,
  Row,
  TextField,
  fmtSize,
  useToast,
} from "../../../ui";
import BookEntries from "./BookEntries";
import ImportResult from "./ImportResult";
import { errorText, matches, pickFiles } from "./common";

export default function Worlds() {
  const toast = useToast();
  const list = useResource("tavernWorlds", R.tavernWorlds);
  const [query, setQuery] = useState("");
  const [importing, setImporting] = useState(false);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [openFile, setOpenFile] = useState<string | null>(null);
  const [removing, setRemoving] = useState<WorldInfo | null>(null);
  const [busy, setBusy] = useState(false);

  const worlds = list.data ?? [];
  const shown = worlds.filter((w) => matches(query, w.name));

  async function importWorlds() {
    const files = await pickFiles("世界书 JSON", ["json"]);
    if (!files.length) return;
    setImporting(true);
    try {
      const r = await api.tavernWorldsImport(files);
      setReport(r);
      invalidate(...AFTER.tavernLibrary);
      if (r.imported.length) toast.ok(`导入了 ${r.imported.length} 本世界书`);
      else toast.error("一本都没导进来，原因在列表上方");
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setImporting(false);
    }
  }

  async function remove() {
    if (!removing) return;
    setBusy(true);
    try {
      const t = await api.tavernWorldDelete(removing.file);
      toast.ok(`「${t.name}」挪进了回收站，在「备份与回收站」里能恢复`);
      invalidate(...AFTER.tavernLibrary);
      setRemoving(null);
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div data-testid="tavern-worlds">
      <div className="tv-toolbar">
        <div className="tv-search">
          <TextField
            label="找世界书"
            value={query}
            placeholder="名字"
            onChange={setQuery}
          />
        </div>
        <span className="tv-toolbar-end">
          <Button
            size="sm"
            icon={<Upload size={12} />}
            loading={importing}
            onClick={() => void importWorlds()}
          >
            导入世界书
          </Button>
          <Button
            size="sm"
            variant="ghost"
            icon={<RefreshCw size={12} />}
            loading={list.loading}
            onClick={() => void list.refresh()}
          >
            刷新
          </Button>
        </span>
      </div>
      <p className="notice mt-1">
        {worlds.length ? `${worlds.length} 本` : ""}
        只收酒馆自己格式的
        JSON；别家的世界书在酒馆里导入（它会转换）。改条目在酒馆里做。
      </p>
      {report && (
        <ImportResult report={report} onClose={() => setReport(null)} />
      )}
      {list.error ? (
        <p className="notice notice--danger mt-2">{list.error}</p>
      ) : list.neverLoaded ? (
        <p className="notice mt-2">读取中…</p>
      ) : worlds.length === 0 ? (
        <EmptyState title="还没有世界书">
          点「导入世界书」，或者在酒馆里建一本。
        </EmptyState>
      ) : shown.length === 0 ? (
        <p className="notice mt-2">没有匹配「{query}」的世界书。</p>
      ) : (
        <div className="tv-rows mt-2">
          {shown.map((w) => (
            <Row
              key={w.file}
              side={
                <span className="flex flex-wrap gap-1.5">
                  <Button
                    size="sm"
                    icon={<Eye size={12} />}
                    disabled={!!w.problem}
                    onClick={() => setOpenFile(w.file)}
                  >
                    看条目
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<Trash2 size={12} />}
                    aria-label={`把「${w.name}」挪进回收站`}
                    onClick={() => setRemoving(w)}
                  />
                </span>
              }
            >
              <span className="min-w-0">
                <strong className="break-all">{w.name}</strong>
                <span className="notice block">
                  {w.problem
                    ? w.problem
                    : `${w.entries} 条（启用 ${w.enabled} 条）· ${fmtSize(w.size)}${w.modified ? ` · 改于 ${w.modified}` : ""}`}
                </span>
              </span>
            </Row>
          ))}
        </div>
      )}
      <Detail file={openFile} onClose={() => setOpenFile(null)} />
      <ConfirmDialog
        open={removing !== null}
        onCancel={() => setRemoving(null)}
        onConfirm={() => void remove()}
        title={`把世界书「${removing?.name ?? ""}」挪进回收站？`}
        confirmLabel="挪进回收站"
        loading={busy}
        danger
      >
        <p>
          文件挪进面板的回收站，在「备份与回收站」里能原样放回去。绑着它的角色卡、聊天在酒馆里会显示「找不到这本世界书」，
          放回去就好了。
        </p>
      </ConfirmDialog>
    </div>
  );
}

function Detail({
  file,
  onClose,
}: {
  file: string | null;
  onClose: () => void;
}) {
  const [detail, setDetail] = useState<WorldDetail | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    setDetail(null);
    setError("");
    if (!file) return;
    let alive = true;
    api
      .tavernWorldDetail(file)
      .then((d) => alive && setDetail(d))
      .catch((e) => alive && setError(errorText(e)));
    return () => {
      alive = false;
    };
  }, [file]);
  return (
    <Modal
      open={file !== null}
      onClose={onClose}
      title={detail?.info.name ?? file ?? ""}
      size="wide"
    >
      <div data-testid="tavern-world-detail">
        {error && <p className="notice notice--danger">{error}</p>}
        {!detail && !error && <p className="notice">正在读这本世界书…</p>}
        {detail && (
          <>
            <p className="notice">
              {detail.info.entries} 条，启用 {detail.info.enabled} 条 ·
              按酒馆里的顺序排
            </p>
            <BookEntries entries={detail.entries} />
          </>
        )}
      </div>
    </Modal>
  );
}

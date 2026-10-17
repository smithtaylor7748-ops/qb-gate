/**
 * 「酒馆」弹窗的「角色卡」分页（2026-09-29，使用者选的「角色卡 / 世界书 / 扩展管理」；第三轮从软件页的卡搬进弹窗）。
 *
 * 看（名字、作者、标签、聊天几段、卡里带的世界书）、导入 PNG 卡、挪进回收站。**改卡在酒馆里做**。
 *
 * - 列表读的是卡里真实写的名字（PNG 的 `ccv3` / `chara` 文本块），读不出来的卡照样列出来、标明原因（不藏）；
 * - 头像用酒馆自己生成的缩略图（`thumbnails\avatar\`），进了可视区才读；没生成过就显示名字的第一个字；
 * - 删除 = 挪进面板的回收站，能恢复；聊天记录默认不跟着走（勾了才一起挪）。
 */
import { useEffect, useRef, useState } from "react";
import {
  BookOpen,
  MessageSquare,
  RefreshCw,
  Trash2,
  Upload,
} from "lucide-react";

import {
  api,
  type CharacterCard,
  type CharacterDetail,
  type ImportReport,
} from "../../../lib/api";
import { AFTER, R } from "../../../lib/resources";
import { invalidate, useResource } from "../../../lib/store";
import {
  Button,
  Checkbox,
  Collapsible,
  ConfirmDialog,
  EmptyState,
  Modal,
  TextField,
  fmtSize,
  useToast,
} from "../../../ui";
import BookEntries from "./BookEntries";
import ImportResult from "./ImportResult";
import { errorText, matches, pickFiles } from "./common";

/** 读过的缩略图（文件名 → data URL；`null` = 酒馆还没生成过）。切分页回来不重读。 */
const thumbs = new Map<string, string | null>();

export default function Characters() {
  const toast = useToast();
  const list = useResource("tavernCharacters", R.tavernCharacters);
  const [query, setQuery] = useState("");
  const [importing, setImporting] = useState(false);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [openFile, setOpenFile] = useState<string | null>(null);

  const cards = list.data ?? [];
  const shown = cards.filter((c) =>
    matches(query, c.name, c.creator, c.tags, c.file),
  );

  async function importCards() {
    const files = await pickFiles("PNG 角色卡", ["png"]);
    if (!files.length) return;
    setImporting(true);
    try {
      const r = await api.tavernCharactersImport(files);
      setReport(r);
      invalidate(...AFTER.tavernLibrary);
      if (r.imported.length) toast.ok(`导入了 ${r.imported.length} 张角色卡`);
      else toast.error("一张都没导进来，原因在列表上方");
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setImporting(false);
    }
  }

  return (
    <div data-testid="tavern-characters">
      <div className="tv-toolbar">
        <div className="tv-search">
          <TextField
            label="找角色"
            value={query}
            placeholder="名字、作者或标签"
            onChange={setQuery}
          />
        </div>
        <span className="tv-toolbar-end">
          <Button
            size="sm"
            icon={<Upload size={12} />}
            loading={importing}
            onClick={() => void importCards()}
          >
            导入角色卡
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
        {cards.length ? `${cards.length} 张` : ""}
        看、导入、挪进回收站在这里；改卡的内容在酒馆里。酒馆开着的话，导入之后在酒馆的角色列表里刷新一下。
      </p>
      {report && (
        <ImportResult report={report} onClose={() => setReport(null)} />
      )}
      {list.error ? (
        <p className="notice notice--danger mt-2">{list.error}</p>
      ) : list.neverLoaded ? (
        <p className="notice mt-2">读取中…</p>
      ) : cards.length === 0 ? (
        <EmptyState title="还没有角色卡">
          点「导入角色卡」选几张 PNG 卡，或者在酒馆里建一个。
        </EmptyState>
      ) : shown.length === 0 ? (
        <p className="notice mt-2">没有匹配「{query}」的角色。</p>
      ) : (
        <div className="tv-char-grid mt-2">
          {shown.map((c) => (
            <CardTile key={c.file} c={c} onOpen={() => setOpenFile(c.file)} />
          ))}
        </div>
      )}
      <Detail file={openFile} onClose={() => setOpenFile(null)} />
    </div>
  );
}

function Avatar({ file, name }: { file: string; name: string }) {
  const ref = useRef<HTMLSpanElement>(null);
  const [src, setSrc] = useState<string | null>(thumbs.get(file) ?? null);
  useEffect(() => {
    if (thumbs.has(file)) return;
    const el = ref.current;
    let alive = true;
    const load = () => {
      api
        .tavernCharacterThumbnail(file)
        .then((d) => {
          thumbs.set(file, d);
          if (alive) setSrc(d);
        })
        .catch(() => thumbs.set(file, null));
    };
    if (!el || typeof IntersectionObserver === "undefined") {
      load();
      return () => {
        alive = false;
      };
    }
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        io.disconnect();
        load();
      }
    });
    io.observe(el);
    return () => {
      alive = false;
      io.disconnect();
    };
  }, [file]);
  return (
    <span ref={ref} className="tv-avatar" aria-hidden="true">
      {src ? (
        <img src={src} alt="" />
      ) : (
        <span>{Array.from(name.trim())[0] ?? "?"}</span>
      )}
    </span>
  );
}

function CardTile({ c, onOpen }: { c: CharacterCard; onOpen: () => void }) {
  return (
    <button type="button" className="tv-char" onClick={onOpen}>
      <Avatar file={c.file} name={c.name} />
      <span className="tv-char-main">
        <span className="tv-char-name">{c.name}</span>
        <span className="tv-char-meta">
          {[
            c.creator,
            c.version && `v${c.version}`,
            c.spec && c.spec.toUpperCase(),
          ]
            .filter(Boolean)
            .join(" · ") || c.file}
        </span>
        {c.problem ? (
          <span className="tv-char-problem">读不出卡数据</span>
        ) : (
          c.tags.length > 0 && (
            <span className="tv-chips">
              {c.tags.slice(0, 3).map((t, i) => (
                <span key={i} className="tv-chip">
                  {t}
                </span>
              ))}
              {c.tags.length > 3 && (
                <span className="tv-chip tv-chip--soft">
                  +{c.tags.length - 3}
                </span>
              )}
            </span>
          )
        )}
        <span className="tv-char-foot">
          <span>
            <MessageSquare size={11} aria-hidden="true" /> {c.chats} 段聊天
          </span>
          {c.book_entries != null && (
            <span>
              <BookOpen size={11} aria-hidden="true" /> 世界书 {c.book_entries}{" "}
              条
            </span>
          )}
        </span>
      </span>
    </button>
  );
}

/** 卡的详情。打开时才读（整张图也在这时候读，超过 8 MB 不带）。 */
function Detail({
  file,
  onClose,
}: {
  file: string | null;
  onClose: () => void;
}) {
  const toast = useToast();
  const [detail, setDetail] = useState<CharacterDetail | null>(null);
  const [error, setError] = useState("");
  const [asking, setAsking] = useState(false);
  const [withChats, setWithChats] = useState(false);
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    setDetail(null);
    setError("");
    setAsking(false);
    setWithChats(false);
    if (!file) return;
    let alive = true;
    api
      .tavernCharacterDetail(file)
      .then((d) => alive && setDetail(d))
      .catch((e) => alive && setError(errorText(e)));
    return () => {
      alive = false;
    };
  }, [file]);

  async function remove() {
    if (!file) return;
    setDeleting(true);
    try {
      const t = await api.tavernCharacterDelete(file, withChats);
      toast.ok(`「${t.name}」挪进了回收站，在「备份与回收站」里能恢复`);
      invalidate(...AFTER.tavernLibrary);
      setAsking(false);
      onClose();
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setDeleting(false);
    }
  }

  const c = detail?.card;
  const texts: [string, string][] = detail
    ? [
        ["描述", detail.description],
        ["性格", detail.personality],
        ["场景", detail.scenario],
        ["示例对话", detail.mes_example],
        ["创作者备注", detail.creator_notes],
        ["系统提示词", detail.system_prompt],
        ["历史后指令", detail.post_history_instructions],
      ]
    : [];

  return (
    <Modal
      open={file !== null}
      onClose={onClose}
      title={c?.name ?? file ?? ""}
      size="wide"
      footer={
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="notice break-all">{file}</span>
          <Button
            variant="danger"
            icon={<Trash2 size={13} />}
            disabled={!detail}
            onClick={() => setAsking(true)}
          >
            挪进回收站
          </Button>
        </div>
      }
    >
      <div data-testid="tavern-character-detail">
        {error && <p className="notice notice--danger">{error}</p>}
        {!detail && !error && <p className="notice">正在读这张卡…</p>}
        {detail && c && (
          <>
            <div className="tv-char-detail">
              {detail.image && (
                <img
                  className="tv-char-image"
                  src={detail.image}
                  alt={`${c.name} 的卡面`}
                />
              )}
              <dl className="tv-facts">
                <dt>作者</dt>
                <dd>{c.creator || "—"}</dd>
                <dt>卡的版本</dt>
                <dd>{c.version || "—"}</dd>
                <dt>格式</dt>
                <dd>{c.spec ? c.spec.toUpperCase() : "—"}</dd>
                <dt>聊天</dt>
                <dd>{c.chats} 段</dd>
                <dt>大小</dt>
                <dd>{fmtSize(c.size)}</dd>
                <dt>改于</dt>
                <dd>{c.modified ?? "—"}</dd>
                {c.tags.length > 0 && (
                  <>
                    <dt>标签</dt>
                    <dd className="tv-chips">
                      {c.tags.map((t, i) => (
                        <span key={i} className="tv-chip">
                          {t}
                        </span>
                      ))}
                    </dd>
                  </>
                )}
              </dl>
            </div>
            {c.problem && (
              <p className="notice notice--warn mt-2">{c.problem}</p>
            )}
            {detail.first_mes && (
              <section className="mt-3">
                <h4 className="tv-sub">开场白</h4>
                <p className="tv-text">{detail.first_mes}</p>
              </section>
            )}
            {detail.alternate_greetings.length > 0 && (
              <Collapsible
                summary={`其他开场白（${detail.alternate_greetings.length}）`}
              >
                {detail.alternate_greetings.map((g, i) => (
                  <p key={i} className="tv-text mt-2">
                    {g}
                  </p>
                ))}
              </Collapsible>
            )}
            {texts
              .filter(([, v]) => v.trim())
              .map(([label, v]) => (
                <Collapsible key={label} summary={label}>
                  <p className="tv-text">{v}</p>
                </Collapsible>
              ))}
            {detail.book.length > 0 && (
              <Collapsible
                summary={`卡里自带的世界书${detail.book_name ? `「${detail.book_name}」` : ""}（${detail.book.length} 条）`}
              >
                <BookEntries entries={detail.book} />
              </Collapsible>
            )}
          </>
        )}
      </div>
      <ConfirmDialog
        open={asking}
        onCancel={() => setAsking(false)}
        onConfirm={() => void remove()}
        title={`把「${c?.name ?? file}」挪进回收站？`}
        confirmLabel="挪进回收站"
        loading={deleting}
        danger
      >
        <p>
          卡文件挪进面板的回收站（
          <code>%LOCALAPPDATA%\ClaudeIpGate\tavern-trash</code>），
          在「备份与回收站」里能原样放回去；清空回收站才真删。
        </p>
        {c && c.chats > 0 && (
          <div className="mt-2">
            <Checkbox checked={withChats} onChange={setWithChats}>
              连同它的 {c.chats} 段聊天记录一起挪
            </Checkbox>
          </div>
        )}
        <p className="notice mt-2">
          酒馆开着、正跟它聊天的话，先在酒馆里切到别的角色 ——
          不然酒馆下一次保存会找不到它。
        </p>
      </ConfirmDialog>
    </Modal>
  );
}

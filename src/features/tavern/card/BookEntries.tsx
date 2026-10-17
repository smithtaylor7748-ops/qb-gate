/**
 * 世界书条目列表：世界书详情与角色卡里自带的世界书共用（2026-09-29）。
 *
 * 只读 —— 改条目在酒馆里做（面板不重写酒馆的编辑界面，`tavern_assets` 那条没变）。
 * 条目多的世界书（几百条）先只画前 200 条，搜索缩小范围再看。
 */
import { useState } from "react";

import type { BookEntry } from "../../../lib/api";
import { Pill, TextField } from "../../../ui";
import { matches } from "./common";

const SHOW = 200;

export default function BookEntries({
  entries,
  searchable = true,
}: {
  entries: BookEntry[];
  searchable?: boolean;
}) {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const hits = entries.filter((e) =>
    matches(query, e.comment, e.keys, e.secondary, e.content),
  );
  return (
    <div className="tv-entries">
      {searchable && entries.length > 6 && (
        <TextField
          label="在条目里找"
          value={query}
          placeholder="标题、关键词或内容里的字"
          onChange={setQuery}
        />
      )}
      {hits.length === 0 ? (
        <p className="notice mt-2">
          {entries.length ? "没有匹配的条目。" : "一条条目都没有。"}
        </p>
      ) : (
        <ol className="tv-entry-list mt-2">
          {hits.slice(0, SHOW).map((e, i) => {
            // uid 来自别人的文件，重复了也不许让 React 报「两条 key 一样」（test:ui 收这一句）。
            const id = `${e.uid}#${i}`;
            const expanded = open === id;
            return (
              <li
                key={id}
                className={`tv-entry${e.disabled ? " tv-entry--off" : ""}`}
              >
                <div className="tv-entry-head">
                  <strong>{e.comment || "（没有标题）"}</strong>
                  {e.constant && <Pill tone="accent">常驻</Pill>}
                  {e.disabled && <Pill>停用</Pill>}
                  {e.order != null && (
                    <span className="notice">顺序 {e.order}</span>
                  )}
                </div>
                {(e.keys.length > 0 || e.secondary.length > 0) && (
                  <div className="tv-chips">
                    {e.keys.map((k, j) => (
                      <span key={"k" + j} className="tv-chip">
                        {k}
                      </span>
                    ))}
                    {e.secondary.map((k, j) => (
                      <span key={"s" + j} className="tv-chip tv-chip--soft">
                        {k}
                      </span>
                    ))}
                  </div>
                )}
                {e.content && (
                  <button
                    type="button"
                    className={`tv-entry-body${expanded ? " is-open" : ""}`}
                    aria-expanded={expanded}
                    onClick={() => setOpen(expanded ? null : id)}
                  >
                    {e.content}
                  </button>
                )}
              </li>
            );
          })}
        </ol>
      )}
      {hits.length > SHOW && (
        <p className="notice mt-2">
          还有 {hits.length - SHOW} 条没画出来 —— 用上面的搜索缩小范围。
        </p>
      )}
    </div>
  );
}

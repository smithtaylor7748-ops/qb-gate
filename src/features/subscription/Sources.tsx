import { Collapsible, ExternalLink } from "../../ui";
import { REVIEWED_ON, SOURCES, sourceById } from "./data";

/** 一行「出处：A · B」，卡片底部用。 */
export function SourceRefs({ ids }: { ids: string[] }) {
  const items = ids.map(sourceById).filter((s) => s !== undefined);
  if (items.length === 0) return null;
  return (
    <p className="qb-sub-refs">
      <span>出处：</span>
      {items.map((s, i) => (
        <span key={s.id}>
          {i > 0 && <span aria-hidden="true"> · </span>}
          <ExternalLink href={s.url}>{s.label}</ExternalLink>
        </span>
      ))}
    </p>
  );
}

/** 整页的来源清单，折叠着放在首页与答疑页底部。 */
export function SourceList() {
  return (
    <Collapsible
      summary={`参考来源（${SOURCES.length} 条，复核于 ${REVIEWED_ON}）`}
    >
      <ul className="qb-sub-sources">
        {SOURCES.map((s) => (
          <li key={s.id}>
            <ExternalLink href={s.url}>{s.label}</ExternalLink>
            {s.note && <span className="qb-sub-source-note">{s.note}</span>}
          </li>
        ))}
      </ul>
      <p className="notice">
        外部页面以各服务商当下显示的为准；价格与实测数字会过时，改数字时一并改复核日期。
      </p>
    </Collapsible>
  );
}

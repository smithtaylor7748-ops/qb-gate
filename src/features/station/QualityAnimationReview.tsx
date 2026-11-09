import { useMemo, useState } from "react";
import { Button } from "../../ui";
import {
  ANIMATION_REVIEW,
  animationPreviewDocument,
} from "../../lib/qualityChecks";

export default function QualityAnimationReview({
  html,
  onReview,
}: {
  html: string;
  onReview: (
    verdict: "passed" | "failed",
    review: Record<string, boolean>,
  ) => void;
}) {
  const [review, setReview] = useState<Record<string, boolean>>({});
  const [narrow, setNarrow] = useState(false);
  const document = useMemo(() => animationPreviewDocument(html), [html]);
  const complete = ANIMATION_REVIEW.every((item) => review[item.id]);
  return (
    <div className="qb-st-form">
      <p className="qb-st-note">
        请观察至少两个动画循环，再切换宽窄预览逐项复核。预览禁止脚本和联网；这里只记录人工观察结果。
      </p>
      <Button onClick={() => setNarrow((v) => !v)}>
        {narrow ? "切换宽屏预览" : "切换窄屏预览"}
      </Button>
      <iframe
        title="动画隔离预览"
        sandbox=""
        referrerPolicy="no-referrer"
        srcDoc={document}
        style={{
          width: narrow ? 320 : "100%",
          maxWidth: "100%",
          height: 420,
          border: "1px solid var(--border, #64748b)",
          background: "#fff",
        }}
      />
      {ANIMATION_REVIEW.map((item) => (
        <label key={item.id} className="qb-st-check">
          <input
            type="checkbox"
            checked={review[item.id] ?? false}
            onChange={(e) =>
              setReview((v) => ({ ...v, [item.id]: e.target.checked }))
            }
          />{" "}
          {item.label}
        </label>
      ))}
      <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
        <Button disabled={!complete} onClick={() => onReview("passed", review)}>
          记录人工复核通过
        </Button>
        <Button
          onClick={() =>
            onReview(
              "failed",
              Object.fromEntries(
                ANIMATION_REVIEW.map((item) => [
                  item.id,
                  review[item.id] ?? false,
                ]),
              ),
            )
          }
        >
          记录人工复核未通过
        </Button>
      </div>
      <details>
        <summary>查看生成的 HTML（本轮内存，不保存正文）</summary>
        <pre
          style={{
            whiteSpace: "pre-wrap",
            overflowWrap: "anywhere",
            maxHeight: 220,
            overflow: "auto",
          }}
        >
          {html}
        </pre>
      </details>
    </div>
  );
}

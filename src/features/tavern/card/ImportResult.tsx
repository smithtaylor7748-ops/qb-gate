/**
 * 导入的结果：进来了几个、没进来的各为什么。角色卡与世界书共用。
 *
 * 没进来的原因要能看全（「不是 PNG 角色卡」「不是酒馆自己的世界书格式」），所以不塞进 toast，
 * 留在分页上，直到下一次导入或者手动关掉。
 */
import { X } from "lucide-react";

import type { ImportReport } from "../../../lib/api";

export default function ImportResult({
  report,
  onClose,
}: {
  report: ImportReport;
  onClose: () => void;
}) {
  return (
    <div
      className={`tv-import ${report.skipped.length ? "tv-import--warn" : "tv-import--ok"}`}
      role="status"
    >
      <div className="tv-import-head">
        <strong>
          导入了 {report.imported.length} 个
          {report.skipped.length ? `，${report.skipped.length} 个没进来` : ""}
        </strong>
        <button
          type="button"
          className="tv-icon-btn"
          aria-label="关掉导入结果"
          onClick={onClose}
        >
          <X size={13} />
        </button>
      </div>
      {report.imported.length > 0 && (
        <p className="notice break-all">进来的：{report.imported.join("、")}</p>
      )}
      {report.skipped.length > 0 && (
        <ul className="tv-import-skipped">
          {report.skipped.map((s, i) => (
            <li key={i}>{s}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

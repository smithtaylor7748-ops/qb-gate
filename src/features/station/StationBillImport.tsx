import { useEffect, useRef, useState } from "react";
import { FileUp } from "lucide-react";
type Summary = {
  model: string;
  group: string;
  currency: string;
  record_count: number;
  cache_hit_rate: number | null;
  cost_total: number | null;
};
type Imported = {
  sources: { source_name: string; record_count: number; groups: Summary[] }[];
};
export default function StationBillImport() {
  const worker = useRef<Worker>();
  const [result, setResult] = useState<Imported | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => () => worker.current?.terminate(), []);
  const read = async (files: FileList | null) => {
    if (!files?.length) return;
    setError("");
    setResult(null);
    setBusy(true);
    worker.current?.terminate();
    try {
      if (
        files.length > 5 ||
        Array.from(files).some((f) => f.size > 5 * 1024 * 1024)
      )
        throw new Error("最多 5 个文件，每个不超过 5 MiB。");
      const input = await Promise.all(
        Array.from(files).map(async (f) => ({
          name: f.name,
          content: await f.arrayBuffer(),
        })),
      );
      const w = new Worker(
        new URL("./station-billing-import-worker.js", import.meta.url),
        { type: "module" },
      );
      worker.current = w;
      w.onmessage = (e) => {
        if (e.data.ok) setResult(e.data.result);
        else setError("文件无法分析：" + String(e.data.error));
        setBusy(false);
        w.terminate();
      };
      w.onerror = () => {
        setError("文件解析失败，请检查 CSV / JSON 格式。");
        setBusy(false);
        w.terminate();
      };
      w.postMessage(
        { files: input },
        input.map((f) => f.content),
      );
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };
  return (
    <details className="audit-import">
      <summary>
        <FileUp size={16} />
        本地账单分析<span>CSV / JSON</span>
      </summary>
      <p>
        按模型和分组汇总缓存、用量与费用。文件只在本机解析，不上传。每个文件最多
        5 MiB、50,000 条记录；原始 quota 不会自动折算为美元。
      </p>
      <label className="audit-file-label">
        {busy ? "正在分析…" : "选择账单文件"}
        <input
          type="file"
          aria-label="选择账单文件"
          accept=".csv,.json"
          multiple
          disabled={busy}
          onChange={(e) => {
            void read(e.target.files);
            e.target.value = "";
          }}
        />
      </label>
      {error && (
        <p role="alert" className="audit-error">
          {error}
        </p>
      )}
      {result?.sources.map((source, i) => (
        <div key={i}>
          <h4>
            {source.source_name} · {source.record_count.toLocaleString()} 条
          </h4>
          <div className="audit-table-scroll">
            <table>
              <thead>
                <tr>
                  <th>模型</th>
                  <th>分组</th>
                  <th>条数</th>
                  <th>缓存占比</th>
                  <th>原始金额 / 单位</th>
                </tr>
              </thead>
              <tbody>
                {source.groups.map((g, j) => (
                  <tr key={j}>
                    <td>{g.model || "未知模型"}</td>
                    <td>{g.group || "未注明"}</td>
                    <td>{g.record_count}</td>
                    <td>
                      {g.cache_hit_rate == null
                        ? "—"
                        : (g.cache_hit_rate * 100).toFixed(1) + "%"}
                    </td>
                    <td>
                      {g.cost_total == null ? "—" : g.cost_total.toFixed(5)}{" "}
                      {g.currency || "单位未注明"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ))}
    </details>
  );
}

import { useState } from "react";
import { api } from "../../lib/api";
import { R } from "../../lib/resources";
import { queryClient, useResource } from "../../lib/store";
import { Button } from "../../ui";

/** IP 锁与执行锁入口共用同一个已保存设置。 */
export default function GateCheckInterval() {
  const settings = useResource("settings", R.settings);
  const saved = settings.data?.gate_check_interval_secs ?? 5;
  const [draft, setDraft] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [success, setSuccess] = useState(false);
  const text = draft ?? String(saved);
  const seconds = Number(text);
  const valid =
    text.trim() !== "" &&
    Number.isInteger(seconds) &&
    seconds >= 1 &&
    seconds <= 30;

  async function save() {
    if (!valid || busy || !settings.data) return;
    setBusy(true);
    setError("");
    setSuccess(false);
    try {
      const next = await api.gateCheckIntervalSave(seconds);
      await queryClient.cancelQueries({ queryKey: ["settings"] });
      queryClient.setQueryData(["settings"], next);
      setDraft(null);
      setSuccess(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="mb-3">
      <div className="flex flex-wrap items-end gap-2">
        <label className="grid gap-1">
          <span className="text-sm">IP 锁检测间隔（秒）</span>
          <input
            type="number"
            min={1}
            max={30}
            step={1}
            className="input w-28"
            value={text}
            disabled={!settings.data || busy}
            onChange={(e) => {
              setDraft(e.target.value);
              setError("");
              setSuccess(false);
            }}
          />
        </label>
        <Button
          loading={busy}
          disabled={!settings.data || !valid || seconds === saved}
          onClick={() => void save()}
        >
          保存间隔
        </Button>
        <span className="text-sm text-[var(--muted)]">
          当前已保存：{saved} 秒
        </span>
      </div>
      <p className="notice mt-2">
        1–30 秒，默认 5 秒。每轮检测完成后等待此时长，所有受管软件共用。
        调大可减少检测请求，但发现 IP 变化会更慢；调小会更快复查。
        保存后运行中的看门狗会重新计算下一次检测时间，无需重启。
      </p>
      {!valid && (
        <p className="notice notice--danger mt-2">请输入 1–30 秒的整数。</p>
      )}
      {(error || settings.error) && (
        <p role="alert" className="notice notice--danger mt-2">
          {error || settings.error}
        </p>
      )}
      {success && (
        <p role="status" className="notice mt-2">
          检测间隔已保存。
        </p>
      )}
    </div>
  );
}

/**
 * Claude 桥接设置的「会话」一段：`/health` 的 `sdk_sessions.sessions`（原来只在 bridge.py 自己的设置页上）。
 * CLI 模式下每条消息起一次 `claude -p`，没有常驻会话 —— 照实这么说，不画一张空表。
 */
import type { BridgeHealth } from "../../../../lib/generated/BridgeHealth";
import { short } from "../format";

export default function Sessions({
  id,
  health,
}: {
  id: string;
  health: BridgeHealth | null;
}) {
  if (!health) return null;
  const sdk = health.invocation_mode === "agent_sdk";
  return (
    <section
      className="bridge-section"
      id={id}
      data-testid="claude-bridge-sessions"
    >
      <h3>会话</h3>
      {!sdk ? (
        <p className="notice">
          现在是 CLI 模式：每条消息起一次 <code>claude -p</code>
          ，没有常驻会话，也就没有可看的 SDK 会话。
        </p>
      ) : health.sessions.length === 0 ? (
        <p className="notice">
          Agent SDK 模式，现在一个会话都没有 ——
          在酒馆里发一句话就会按聊天建一个。
        </p>
      ) : (
        <div className="qb-acct-tw">
          <table className="ag-table whitespace-nowrap">
            <thead>
              <tr>
                <th>聊天</th>
                <th>状态</th>
                <th>最近使用</th>
                <th>本轮输入 · 输出 token</th>
                <th>缓存读 · 写</th>
                <th>缓存说明</th>
              </tr>
            </thead>
            <tbody>
              {health.sessions.map((s, i) => (
                <tr key={`${s.chat_instance_id || s.sdk_session_id}#${i}`}>
                  <td className="mono">{s.chat_instance_id || "—"}</td>
                  <td>{s.status || "—"}</td>
                  <td className="mono">{s.last_used_at || "—"}</td>
                  <td className="mono">
                    {short(s.input_tokens)} · {short(s.output_tokens)}
                  </td>
                  <td className="mono">
                    {short(s.cache_read)} · {short(s.cache_write)}
                  </td>
                  <td>{s.cache_message || "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

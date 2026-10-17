/**
 * 一条调用的全文（系统提示词、送进去的消息、回复）。**展开时才读**（桥自己的监控页也是这样），读不到就照实说。
 * 里面是使用者的聊天内容：只在本机显示，不落盘、不进日志。
 */
import { useEffect, useState } from "react";

import { api } from "../../../../lib/api";
import type { BridgeCallDetail } from "../../../../lib/generated/BridgeCallDetail";
import { errorText } from "../../card/common";
import { count } from "../format";

export default function CallDetail({ id }: { id: string }) {
  const [detail, setDetail] = useState<BridgeCallDetail | null>(null);
  const [err, setErr] = useState("");
  useEffect(() => {
    let live = true;
    api
      .tavernBridgeCallDetail(id)
      .then((d) => live && setDetail(d))
      .catch((e) => live && setErr(errorText(e)));
    return () => {
      live = false;
    };
  }, [id]);
  if (err) return <p className="notice notice--danger">{err}</p>;
  if (!detail) return <p className="notice">正在读取全文…</p>;
  if (detail.raw)
    return (
      <div className="bridge-call-detail">
        <p className="notice">这条记录的形状认不出来，原样显示：</p>
        <pre>{detail.raw}</pre>
      </div>
    );
  return (
    <div className="bridge-call-detail" data-testid="claude-call-detail">
      {detail.error && (
        <p className="notice notice--danger">出错：{detail.error}</p>
      )}
      {detail.system_prompt && (
        <details>
          <summary>
            系统提示词（{count(detail.system_prompt.length)} 字符）
          </summary>
          <pre>{detail.system_prompt}</pre>
        </details>
      )}
      {detail.messages.map((m, i) => (
        <div key={i} className="bridge-call-message">
          <span className="bridge-call-role">{m.role}</span>
          <pre>{m.content}</pre>
        </div>
      ))}
      <div className="bridge-call-message bridge-call-message--reply">
        <span className="bridge-call-role">回复</span>
        <pre>{detail.output ?? "（没有回复正文）"}</pre>
      </div>
    </div>
  );
}

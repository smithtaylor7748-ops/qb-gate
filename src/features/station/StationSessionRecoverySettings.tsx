import type { RouterSettings } from "../../lib/generated/RouterSettings";

export function StationSessionRecoverySettings({
  settings,
  onChange,
}: {
  settings: RouterSettings;
  onChange: (patch: Partial<RouterSettings>) => void;
}) {
  if (settings.client !== "codex") return null;
  const recovery = settings.session_recovery;
  return (
    <section aria-label="Codex 会话修复" className="qb-st-admission-settings">
      <h4 className="qb-st-sect">Codex 会话修复</h4>
      <label className="qb-st-admission-check">
        <input
          type="checkbox"
          checked={recovery.strip_item_ids}
          onChange={(event) =>
            onChange({
              session_recovery: {
                ...recovery,
                strip_item_ids: event.target.checked,
              },
            })
          }
        />
        清理历史条目的上游 ID
      </label>
      <p className="qb-st-note">
        默认关闭。切换中转线路后，旧会话因消息或工具条目 ID
        无法识别而报错时可启用。 仅处理经过本机路由的 Codex Responses
        请求；保留正文、工具调用关系、phase 和业务 ID，不修改本地会话文件。
      </p>
      <label className="qb-st-admission-check">
        <input
          type="checkbox"
          checked={recovery.filter_encrypted_content}
          onChange={(event) =>
            onChange({
              session_recovery: {
                ...recovery,
                filter_encrypted_content: event.target.checked,
              },
            })
          }
        />
        过滤无法复用的加密推理状态
      </label>
      <p className="qb-st-note">
        独立开关，默认关闭。用于 invalid_encrypted_content
        一类错误；保留可读推理摘要，但移除密文可能影响推理连续性。
        依赖服务端历史引用、纯加密压缩、工具或子代理加密内容时会停止发送并说明原因。
        需要请求携带完整历史，无法补回客户端没有发送的上下文。
      </p>
    </section>
  );
}

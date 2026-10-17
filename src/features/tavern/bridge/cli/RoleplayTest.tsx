/**
 * GPT / Gemini 桥接设置里的「角色扮演链路」测试：真的经这条桥问一句，看回不回得来、像不像那个角色。
 * **会用掉这一家的额度**（一次短对话）—— 按钮旁边写着。
 */
import { useState } from "react";

import { api } from "../../../../lib/api";
import type { TavernRoleplayTest } from "../../../../lib/generated/TavernRoleplayTest";
import { Button, Pill, useToast } from "../../../../ui";
import { errorText } from "../../card/common";
import { META, type CliProvider } from "./meta";

export default function RoleplayTest({
  provider,
  id,
}: {
  provider: CliProvider;
  id: string;
}) {
  const toast = useToast();
  const meta = META[provider];
  const [fixture, setFixture] = useState(
    "你叫林澈，是一名温和但机敏的图书管理员。请始终保持角色身份，用中文回应。",
  );
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<TavernRoleplayTest | null>(null);

  async function run() {
    setTesting(true);
    try {
      const r = await api.tavernBridgeRoleplayTest(provider, fixture);
      setResult(r);
      if (r.ok) toast.ok(`${meta.name}角色扮演测试通过`);
      else toast.error(r.detail);
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setTesting(false);
    }
  }

  return (
    <div className="tavern-roleplay-test" id={id}>
      <div className="tavern-roleplay-head">
        <strong>{meta.testTitle}</strong>
        <Pill tone={result ? (result.ok ? "ok" : "danger") : "default"}>
          {result ? (result.ok ? "通过" : "失败") : "未测试"}
        </Pill>
      </div>
      <textarea
        className="tavern-roleplay-fixture"
        rows={2}
        value={fixture}
        onChange={(e) => setFixture(e.target.value)}
        aria-label={`${meta.name}角色扮演测试提示词`}
        data-testid={`${provider}-roleplay-fixture`}
      />
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          loading={testing}
          onClick={() => void run()}
          data-testid={`${provider}-roleplay-test`}
        >
          测试真实桥接
        </Button>
        <span className="notice">
          真的经这条桥问一句，会用掉一点这一家的额度。
        </span>
      </div>
      {result && (
        <p className="notice mt-1">
          {[
            result.model && `模型 ${result.model}`,
            result.latency_ms != null &&
              `${(result.latency_ms / 1000).toFixed(1)} 秒`,
            result.output_tokens != null &&
              `输出 ${result.output_tokens} token`,
          ]
            .filter(Boolean)
            .join(" · ") || result.detail}
        </p>
      )}
      {result?.reply && <p className="tavern-roleplay-reply">{result.reply}</p>}
    </div>
  );
}

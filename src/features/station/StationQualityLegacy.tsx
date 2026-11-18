/**
 * 旧 turn-state 接管的恢复入口（GPT 质量检查弹窗里，只在检测到旧接管时出现）。
 *
 * 2026-11-15 起这里原来那套「原五题手动导入与历史记录」删了（E4）：它对应的 turn-state 长度筛选与注入
 * 早已停用，手动导入五题只是把一份固定题组的答案粘回来打分，跟上面的「GPT 质量检查」重复，
 * 而且「清除测试记录」一点就删（A5）。旧记录（`localStorage` 的 `qb-quality-check-v1`）不动、不读。
 *
 * ⛔ 恢复能力本身**必须留着**（仓库 CLAUDE.md「Codex 质量检查与旧 turn-state 接管恢复」）：
 * 面板启动时会自动按 marker 恢复，恢复失败要能在这里重试。
 */

import { useEffect, useState } from "react";
import { Button } from "../../ui";
import { stationApi } from "../../lib/station";

export default function StationQualityLegacy() {
  const [legacy, setLegacy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  useEffect(() => {
    let active = true;
    stationApi
      .routerStatus("codex")
      .then((s) => {
        if (active)
          setLegacy(
            s.turnstate_official_armed ||
              s.turnstate_takeover !== null ||
              s.turnstate_enabled,
          );
      })
      .catch((e: unknown) => {
        if (active) setNotice(`旧配置状态读取失败：${String(e)}`);
      });
    return () => {
      active = false;
    };
  }, []);
  const restore = async () => {
    setBusy(true);
    try {
      await stationApi.turnstateDisable();
      setLegacy(false);
      setNotice(
        "旧 turn-state 接管已恢复。已使用该入口的 Codex 请自行重启后继续。",
      );
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBusy(false);
    }
  };
  if (!legacy && !notice) return null;
  return (
    <div className="qb-st-form">
      {legacy && (
        <div className="qb-st-nudge">
          <p>
            检测到旧 turn-state 接管。恢复会撤销旧路由配置；使用该入口的 Codex
            需自行重启。
          </p>
          <Button disabled={busy} onClick={() => void restore()}>
            恢复旧 turn-state 配置
          </Button>
        </div>
      )}
      {notice && (
        <p role="status" className="qb-st-note">
          {notice}
        </p>
      )}
    </div>
  );
}

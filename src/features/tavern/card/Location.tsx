/**
 * 「位置与端口」：酒馆目录、启动脚本、酒馆端口（三家共用）+ 改用别处的酒馆（定位 / 手填）。
 *
 * 原来是 `SillyTavernCard.tsx` 里的一个内部组件，2026-09-29 分页重构时搬出来（第二轮在「酒馆」弹窗的概览里，
 * 第三轮回到软件页的 SillyTavern 卡 —— 装在哪、用哪一份是「软件」的事），行为没变：
 * ⛔ 只存这一处的几个字段（`saveTavernFields`）—— 同一份酒馆配置三条桥的设置（软件页三张 CLI 卡最下面）也在改。
 */
import { useState } from "react";
import { Radar } from "lucide-react";

import { api, type TavernCandidate } from "../../../lib/api";
import { AFTER, R } from "../../../lib/resources";
import { invalidate, useResource } from "../../../lib/store";
import {
  Button,
  Collapsible,
  PathField,
  PortField,
  Row,
  useToast,
} from "../../../ui";
import { pick, saveTavernFields, type TavernPatch } from "../saveTavernConfig";
import { errorText } from "./common";

export default function Location({
  root,
  launcher,
  configPort,
}: {
  root: string;
  launcher: string;
  configPort: number | null;
}) {
  const toast = useToast();
  const cfgRes = useResource("tavernConfig", R.tavernConfig);
  const cfg = cfgRes.data ?? null;
  const [draft, setDraft] = useState<TavernPatch>({});
  const [saving, setSaving] = useState(false);
  const [locating, setLocating] = useState("");
  const [found, setFound] = useState<{
    sillytavern: TavernCandidate[];
    launcher: TavernCandidate[];
  } | null>(null);
  const dirty = Object.keys(draft).length > 0;
  const stPort = pick(draft, cfg, "st_port") ?? 8000;

  async function save() {
    setSaving(true);
    try {
      await saveTavernFields(draft);
      setDraft({});
      invalidate(...AFTER.tavernSetup);
      toast.ok("已保存");
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setSaving(false);
    }
  }

  async function locate(deep: boolean) {
    setLocating(deep ? "deep" : "quick");
    try {
      const s = await api.tavernLocate(deep);
      setFound({ sillytavern: s.sillytavern, launcher: s.launcher });
      if (!s.sillytavern.length && !s.launcher.length)
        toast.error(
          s.truncated
            ? `扫了 ${s.scanned_dirs} 个目录还没扫完，也没找到。可以试深扫，或自己填。`
            : "本机没找到别的酒馆。",
        );
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setLocating("");
    }
  }

  return (
    <section className="tv-block" aria-labelledby="tv-location">
      <h3 id="tv-location" className="tv-block-title">
        位置与端口
      </h3>
      <Row>
        <span>目录</span>
        <span className="break-all font-mono text-xs">{root || "没配"}</span>
      </Row>
      <Row>
        <span>启动脚本</span>
        <span className="break-all font-mono text-xs">
          {launcher || "没配"}
        </span>
      </Row>
      <div className="mt-2 max-w-xs">
        <PortField
          label="酒馆端口（三条桥共用这一个酒馆）"
          value={stPort}
          onChange={(v) => setDraft({ ...draft, st_port: v })}
        />
      </div>
      {configPort != null && configPort !== stPort && (
        <p className="notice notice--warn mt-1">
          它自己的 config.yaml 写的是 {configPort}，面板在 {stPort} 上等它 ——
          对不上的话启动会一直等不到。
        </p>
      )}
      <Collapsible className="mt-2" summary="改用别处的酒馆">
        <p className="notice">
          指到你自己的那份（git
          克隆的、解压的都行）。面板只起、只停它，不改它的程序文件；
          更新、换版本、卸载只对面板装的那份。
        </p>
        <div className="mt-2 flex flex-wrap gap-2">
          <Button
            size="sm"
            icon={<Radar size={12} />}
            loading={locating === "quick"}
            disabled={!!locating}
            onClick={() => void locate(false)}
          >
            自动定位
          </Button>
          <Button
            size="sm"
            variant="ghost"
            loading={locating === "deep"}
            disabled={!!locating}
            onClick={() => void locate(true)}
          >
            深扫
          </Button>
        </div>
        {found &&
          (
            [
              ["sillytavern_root", found.sillytavern],
              ["st_launcher", found.launcher],
            ] as const
          ).map(([key, list]) =>
            list.slice(0, 3).map((c) => (
              <div
                key={key + c.path}
                className={`tv-cand mt-1${pick(draft, cfg, key) === c.path ? " tv-cand--picked" : ""}`}
              >
                <span className="tv-cand-main">
                  <span className="tv-cand-path">{c.path}</span>
                  <span className="tv-cand-note">{c.note}</span>
                </span>
                <Button
                  size="sm"
                  disabled={pick(draft, cfg, key) === c.path}
                  onClick={() => setDraft({ ...draft, [key]: c.path })}
                >
                  {pick(draft, cfg, key) === c.path ? "已选" : "采用"}
                </Button>
              </div>
            )),
          )}
        <div className="mt-2 flex flex-col gap-2">
          <PathField
            label="SillyTavern 目录"
            value={pick(draft, cfg, "sillytavern_root") ?? ""}
            onChange={(v) => setDraft({ ...draft, sillytavern_root: v })}
          />
          <PathField
            label="酒馆启动脚本"
            kind="file"
            value={pick(draft, cfg, "st_launcher") ?? ""}
            onChange={(v) => setDraft({ ...draft, st_launcher: v })}
          />
        </div>
      </Collapsible>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="primary"
          loading={saving}
          disabled={!dirty || saving}
          onClick={() => void save()}
        >
          保存
        </Button>
        <Button
          size="sm"
          disabled={!dirty || saving}
          onClick={() => setDraft({})}
        >
          放弃改动
        </Button>
        {dirty && <span className="tv-dirty">改动还没保存</span>}
      </div>
    </section>
  );
}

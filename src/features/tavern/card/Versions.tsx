/**
 * 软件页 SillyTavern 卡的「版本」（2026-09-29，使用者选的「版本回退与指定版本」）。
 * 第二轮时是卡上的一个分页；第三轮别的分页搬进了账户页的「酒馆」弹窗，这一块留在卡上、不再分页。
 *
 * - **现在**：装着哪一版、是不是面板装的、留着的那一版（能「换回」）。
 * - **GitHub 上**：最新的正式版（点了才问）；最近的正式版清单（点了才读发布页的 Atom），每一版能展开看更新说明，
 *   能「换到这一版」（面板装的那份）或「装这一版」（还没有面板装的那份时）；清单里没有的标签可以手填。
 * - `children`：卡上排在「卸载」前面的那一块（「位置与端口」）。
 * - **卸载**：只对面板装的那份，排最后。
 *
 * ⛔ 两条：联网只在点了之后（没有定时器）；你自己的那份（git 克隆的）面板不替它换版本 —— 按钮不给，写明去哪做。
 */
import { useState, type ReactNode } from "react";
import { History, ListTree } from "lucide-react";

import {
  api,
  type TavernRelease,
  type TavernReleaseNote,
  type TavernSetupStatus,
} from "../../../lib/api";
import { RELATION_TEXT, relation } from "../../../lib/tavernVersions";
import {
  Button,
  Collapsible,
  ExternalLink,
  Pill,
  TextField,
} from "../../../ui";
import { errorText } from "./common";
import { UninstallButton, type SetupController } from "./SetupDialogs";

export default function Versions({
  s,
  setup,
  taskRunning,
  children,
}: {
  s: TavernSetupStatus | undefined;
  setup: SetupController;
  taskRunning: boolean;
  /** 排在「卸载」前面、占满一行的那一块。 */
  children?: ReactNode;
}) {
  const [latest, setLatest] = useState<TavernRelease | null>(null);
  const [latestErr, setLatestErr] = useState("");
  const [asking, setAsking] = useState(false);
  const [list, setList] = useState<TavernReleaseNote[] | null>(null);
  const [listErr, setListErr] = useState("");
  const [listing, setListing] = useState(false);
  const [manual, setManual] = useState("");
  const [manualRel, setManualRel] = useState<TavernRelease | null>(null);
  const [manualErr, setManualErr] = useState("");
  const [checking, setChecking] = useState(false);

  const managed = s?.managed ?? null;
  const blocked = !s || !!setup.busy || taskRunning;

  async function checkLatest() {
    setAsking(true);
    setLatestErr("");
    try {
      setLatest(await api.tavernLatest());
    } catch (e) {
      setLatestErr(errorText(e));
    } finally {
      setAsking(false);
    }
  }

  async function listReleases() {
    setListing(true);
    setListErr("");
    try {
      setList(await api.tavernReleases());
    } catch (e) {
      setListErr(errorText(e));
    } finally {
      setListing(false);
    }
  }

  async function checkManual() {
    setChecking(true);
    setManualErr("");
    setManualRel(null);
    try {
      setManualRel(await api.tavernRelease(manual.trim()));
    } catch (e) {
      setManualErr(errorText(e));
    } finally {
      setChecking(false);
    }
  }

  /** 某一版的动作按钮：面板装的那份 → 换过去；还没有面板装的那份 → 装它；你自己的那份 → 不给。 */
  function action(tag: string, version: string) {
    const rel = relation(version, s?.version);
    if (managed)
      return (
        <Button
          size="sm"
          variant={rel === "older" ? "danger" : "default"}
          disabled={blocked || rel === "same"}
          onClick={() => setup.open("update", tag)}
        >
          {rel === "same"
            ? "就是这一版"
            : rel === "older"
              ? "降级到这一版"
              : "换到这一版"}
        </Button>
      );
    if (!s?.present)
      return (
        <Button
          size="sm"
          disabled={blocked}
          onClick={() => setup.open("install", tag)}
        >
          装这一版
        </Button>
      );
    return (
      <Button
        size="sm"
        disabled={blocked}
        onClick={() => setup.open("install", tag)}
      >
        另装一份面板托管的
      </Button>
    );
  }

  return (
    <div className="tv-versions">
      <section className="tv-block" aria-labelledby="tv-now">
        <h3 id="tv-now" className="tv-block-title">
          现在
        </h3>
        {!s ? (
          <p className="notice">读取中…</p>
        ) : !s.present ? (
          <p className="notice">
            还没有酒馆。右上角「下载安装」从官方 GitHub
            装最新的正式版，或者在下面挑一个版本装。
          </p>
        ) : (
          <>
            <p>
              SillyTavern{" "}
              <strong className="mono">{s.version ?? "版本读不出"}</strong>
              {managed ? (
                <span className="notice">
                  {" "}
                  · 面板装的（标签 {managed.tag}
                  {managed.installed_at
                    ? `，装于 ${managed.installed_at.slice(0, 10)}`
                    : ""}
                  ）
                </span>
              ) : (
                <span className="notice"> · 你自己的那份</span>
              )}
            </p>
            {!managed && (
              <p className="notice mt-1">
                不是面板装的：面板只起、只停它，不替它换版本 ——
                用它自己的方式（例如 git pull / git checkout
                某个标签）。想让面板管版本，就「下载安装」一份面板托管的（可以把这份的数据复制过去，这份不动）。
              </p>
            )}
            {managed && (
              <div className="tv-kept mt-2">
                {s.previous_version ? (
                  <>
                    <span>
                      留着的那一版：
                      <strong className="mono">{s.previous_version}</strong>
                      <span className="notice">
                        {" "}
                        （
                        {RELATION_TEXT[
                          relation(s.previous_version, s.version)
                        ] || "版本读不出"}
                        ； 在 sillytavern.previous，上一次换版本时整份留下的）
                      </span>
                    </span>
                    <Button
                      size="sm"
                      icon={<History size={12} />}
                      disabled={blocked}
                      onClick={() => setup.open("swap")}
                      data-testid="tavern-swap-open"
                    >
                      换回 {s.previous_version}
                    </Button>
                  </>
                ) : (
                  <span className="notice">
                    没有留着的那一版（换过一次版本之后，原来那份会整份留在
                    sillytavern.previous，这里就能换回去）。
                  </span>
                )}
              </div>
            )}
          </>
        )}
      </section>

      <section className="tv-block" aria-labelledby="tv-latest">
        <h3 id="tv-latest" className="tv-block-title">
          GitHub 上
        </h3>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            loading={asking}
            disabled={asking}
            onClick={() => void checkLatest()}
          >
            检查新版本
          </Button>
          {latest && (
            <span className="notice">
              最新正式版 <strong className="mono">{latest.version}</strong>
              {s?.version
                ? `（${RELATION_TEXT[relation(latest.version, s.version)]}）`
                : ""}{" "}
              · <ExternalLink href={latest.page_url}>发布页</ExternalLink>
            </span>
          )}
          {latest &&
            managed &&
            relation(latest.version, s?.version) === "newer" && (
              <Button
                size="sm"
                variant="primary"
                disabled={blocked}
                onClick={() => setup.open("update")}
              >
                更新到 {latest.version}
              </Button>
            )}
        </div>
        {latestErr && <p className="notice notice--danger mt-1">{latestErr}</p>}
        <p className="notice mt-1">
          只从 SillyTavern 官方 GitHub 取；点了才问，没有定时检查。
        </p>
      </section>

      <section className="tv-block tv-wide" aria-labelledby="tv-history">
        <h3 id="tv-history" className="tv-block-title">
          指定版本
        </h3>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            icon={<ListTree size={12} />}
            loading={listing}
            disabled={listing}
            onClick={() => void listReleases()}
            data-testid="tavern-releases-list"
          >
            {list ? "重新读取" : "列出最近的正式版"}
          </Button>
          <span className="notice">
            读发布页自带的清单（最近 10 个），不走 GitHub API。
          </span>
        </div>
        {listErr && <p className="notice notice--danger mt-1">{listErr}</p>}
        {list && (
          <ul className="tv-releases mt-2" data-testid="tavern-releases">
            {list.map((r) => {
              const rel = relation(r.version, s?.version);
              return (
                <li key={r.tag} className="tv-release">
                  <div className="tv-release-head">
                    <strong className="mono">{r.version}</strong>
                    <span className="notice">更新于 {r.date || "?"}</span>
                    {rel !== "unknown" && (
                      <Pill
                        tone={
                          rel === "same"
                            ? "ok"
                            : rel === "newer"
                              ? "accent"
                              : "default"
                        }
                      >
                        {RELATION_TEXT[rel]}
                      </Pill>
                    )}
                    <span className="tv-release-actions">
                      <ExternalLink href={r.page_url}>发布页</ExternalLink>
                      {action(r.tag, r.version)}
                    </span>
                  </div>
                  {r.notes && (
                    <Collapsible summary="更新说明">
                      <pre className="tv-notes">{r.notes}</pre>
                    </Collapsible>
                  )}
                </li>
              );
            })}
          </ul>
        )}
        <div className="tv-manual mt-3">
          <TextField
            label="清单里没有的：手填标签"
            value={manual}
            placeholder="例如 1.12.14"
            onChange={(v) => {
              setManual(v);
              setManualRel(null);
              setManualErr("");
            }}
          />
          <Button
            size="sm"
            loading={checking}
            disabled={checking || !manual.trim()}
            onClick={() => void checkManual()}
          >
            核对
          </Button>
        </div>
        {manualErr && <p className="notice notice--danger mt-1">{manualErr}</p>}
        {manualRel && (
          <div className="tv-release mt-2">
            <div className="tv-release-head">
              <strong className="mono">{manualRel.version}</strong>
              <span className="notice">
                要求 Node.js {manualRel.node_engine ?? "（读不到）"}
              </span>
              <span className="tv-release-actions">
                <ExternalLink href={manualRel.page_url}>发布页</ExternalLink>
                {action(manualRel.tag, manualRel.version)}
              </span>
            </div>
          </div>
        )}
      </section>

      {children && <div className="tv-wide">{children}</div>}

      {managed && (
        <section
          className="tv-block tv-block--danger tv-wide"
          aria-labelledby="tv-remove"
        >
          <h3 id="tv-remove" className="tv-block-title">
            卸载
          </h3>
          <p className="notice">
            先把你的数据整份备份，再删面板装的这份和留着的那一版。你自己的那份酒馆面板一个字都不删。
          </p>
          <div className="mt-2">
            <UninstallButton
              disabled={blocked}
              onClick={() => setup.open("uninstall")}
            />
          </div>
        </section>
      )}
    </div>
  );
}

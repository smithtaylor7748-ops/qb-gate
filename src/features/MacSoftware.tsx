import { useState } from "react";
import { api, type InstallTarget, type Software } from "../lib/api";
import { antigravityApi } from "../lib/antigravity";
import { R } from "../lib/resources";
import { useResource } from "../lib/store";
import { useAction } from "../lib/workspace";
import { Button, ExternalLink } from "../ui";

export default function MacSoftware() {
  const software = useResource("software", R.software);
  const action = useAction();
  const [installing, setInstalling] = useState<InstallTarget | "gemini" | null>(
    null,
  );
  const data = software.data;
  const install = async (target: InstallTarget) => {
    setInstalling(target);
    try {
      await action.run(
        "mac-install",
        () => api.installRun(target),
        "安装任务完成",
      );
      await software.refresh();
    } finally {
      setInstalling(null);
    }
  };
  const installGemini = async () => {
    setInstalling("gemini");
    try {
      await action.run(
        "mac-gemini",
        () => antigravityApi.geminiCliInstall(),
        "安装任务完成",
      );
      await software.refresh();
    } finally {
      setInstalling(null);
    }
  };
  const items: Array<{
    label: string;
    software?: Software;
    target?: InstallTarget;
    url?: string;
  }> = [
    { label: "Claude Code", software: data?.claudeCode, target: "claude-code" },
    { label: "Codex CLI", software: data?.codex, target: "codex" },
    {
      label: "Claude 桌面端",
      software: data?.claudeDesktop,
      url: "https://claude.ai/download",
    },
    {
      label: "Codex 桌面端",
      software: data?.codexDesktop,
      url: "https://chatgpt.com/codex",
    },
    { label: "Gemini CLI", software: data?.geminiCli },
    {
      label: "Antigravity",
      software: data?.antigravityIde,
      url: "https://antigravity.google/download",
    },
  ];
  return (
    <section>
      <header className="qb-page-heading">
        <div>
          <h1>软件与安装</h1>
          <p>
            Apple Silicon · 安装命令行工具，连接 Applications 中的桌面应用。
          </p>
        </div>
        <Button
          loading={software.loading}
          onClick={() => void software.refresh()}
        >
          重新检测
        </Button>
      </header>
      <p className="notice mb-3">
        桌面应用请从官方渠道安装到 Applications。命令行工具安装到 QB Gate
        的托管目录；首次登录由各工具自己的登录界面完成。
      </p>
      {items.map(({ label, software: item, target, url }) => (
        <section className="qb-setting-row" key={label}>
          <span>
            <strong>{label}</strong>
            <small>
              {item?.installed
                ? `${item.version ?? "已安装"} · ${item.path}`
                : "尚未检测到"}
            </small>
            {item?.advisory && <small>{item.advisory}</small>}
            {label === "Gemini CLI" && (
              <small>需要已安装 Node.js LTS 与 npm。</small>
            )}
          </span>
          {target ? (
            <Button
              loading={installing === target}
              disabled={!!action.pending}
              onClick={() => void install(target)}
            >
              {item?.installed ? "更新" : "安装"}
            </Button>
          ) : url ? (
            <ExternalLink href={url}>官方下载</ExternalLink>
          ) : (
            <Button
              loading={installing === "gemini"}
              disabled={!!action.pending}
              onClick={() => void installGemini()}
            >
              {item?.installed ? "更新" : "安装"}
            </Button>
          )}
        </section>
      ))}
      {(software.error || action.error) && (
        <p className="notice notice--danger" role="alert">
          {software.error || action.error}
        </p>
      )}
    </section>
  );
}

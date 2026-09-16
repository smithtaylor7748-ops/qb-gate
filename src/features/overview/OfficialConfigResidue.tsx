/**
 * 官方目录里的 API 配置残留：先看差异，再迁进独立的中转环境。
 *
 * # 它为什么在总览上，而不是在一个「管理」页里
 *
 * 0.20.0 之前它住在 `/accounts`（`Official.tsx`）。那一页是上一代设计留下的：
 * 从总览点「管理」跳过去，看到的却是比小条条还少的信息，而**使用者要的
 * 「管理账户」就是删除**。删除现在直接做在小条条上，那一页也就没有理由存在了 ——
 * 于是这一段搬回总览，`/accounts` 重定向到 `/`。
 *
 * 全项目只有这一处能做这件事（`official_preview` / `official_migrate`），
 * 所以搬家时一行都不能丢。
 *
 * 默认折叠：它是低频操作，而且要先停掉相关官方会话才能做。
 */

import { useState } from "react";

import {
  useAction,
  workspaceApi,
  CLIENT_NAMES,
  type Client,
  type ConfigPreview,
} from "../../lib/workspace";
import { Button, Modal } from "../../ui";

export default function OfficialConfigResidue({
  activeLabel,
}: {
  /** 当前槽位。Codex 不吃它 —— 它用自己的原生登录目录。 */
  activeLabel: string;
}) {
  const action = useAction();
  const [inspecting, setInspecting] = useState(false);
  const [migration, setMigration] = useState<{
    client: Client;
    id: string;
    files: ConfigPreview[];
  } | null>(null);

  return (
    <>
      <Button size="sm" variant="ghost" onClick={() => setInspecting(true)}>
        检查官方目录的 API 配置
      </Button>
      <Modal
        open={inspecting}
        onClose={() => setInspecting(false)}
        title="检查官方目录的 API 配置"
      >
        <p className="notice">
          官方客户端的配置目录里如果留着 API
          认证或第三方端点，这个账户走的其实不是官方身份。
          这里可以先看差异，再把它们迁进独立的中转环境。
          <strong>请先停掉相关官方会话</strong>
          ；认不出来的云平台配置要自己处理。
        </p>
        <div className="mt-2 flex flex-wrap gap-2">
          {(["claude-code", "codex"] as Client[]).map((client) => (
            <Button
              key={client}
              size="sm"
              loading={action.pending === "preview"}
              disabled={!!action.pending}
              onClick={() =>
                void action
                  .run("preview", () =>
                    workspaceApi.officialPreview(
                      client,
                      client === "codex" ? "" : activeLabel,
                    ),
                  )
                  .then((files) => {
                    if (files)
                      setMigration({
                        client,
                        id: client === "codex" ? "" : activeLabel,
                        files,
                      });
                  })
              }
            >
              检查 {CLIENT_NAMES[client]}
            </Button>
          ))}
        </div>
        {action.error && (
          <p role="alert" className="notice notice--danger mt-2">
            {action.error}
          </p>
        )}
      </Modal>

      <Modal
        open={!!migration}
        onClose={() => setMigration(null)}
        title="迁移 API 配置"
        size="wide"
        footer={
          <>
            <Button onClick={() => setMigration(null)}>取消</Button>
            <Button
              variant="primary"
              loading={action.pending === "migrate"}
              disabled={!migration?.files.length}
              onClick={() => {
                if (migration)
                  void action
                    .run(
                      "migrate",
                      () =>
                        workspaceApi.officialMigrate(
                          migration.client,
                          migration.id,
                          migration.files[0]?.fingerprint ?? "",
                        ),
                      "已迁移到独立中转环境",
                    )
                    .then((done) => {
                      if (done) setMigration(null);
                    });
              }}
            >
              创建中转环境并移除上述残留
            </Button>
          </>
        }
      >
        {migration?.files.length === 0 && (
          <p className="notice">
            这个目录里没有查到 API 配置残留 —— 它走的是官方身份。
          </p>
        )}
        {migration?.files.map((file) => (
          <section key={file.path}>
            <p className="qb-break">{file.path}</p>
            <div className="qb-diff">
              <pre>{file.before}</pre>
              <pre>{file.after || "移除 API 认证文件"}</pre>
            </div>
          </section>
        ))}
      </Modal>
    </>
  );
}

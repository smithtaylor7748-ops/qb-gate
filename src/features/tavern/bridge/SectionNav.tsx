/**
 * 桥接设置顶上那一排「跳到」（2026-09-29；第三轮起桥接设置在软件页各自那张 CLI 卡的最下面）。
 *
 * Claude 那份一屏装不下（连接 · 七项设置 · 会话 · 一张八列的调用日志），原来只能一路往下滚找。
 * 这里不做分页 —— 分页会把「别家的字段一项都不许出现」那条检查（test:ui）变成只查当前那一页 ——
 * 而是钉在顶上的一排锚点，点了滚到那一段。
 */
export interface Section {
  id: string;
  label: string;
}

export default function SectionNav({ sections }: { sections: Section[] }) {
  return (
    <nav className="bridge-nav" aria-label="跳到">
      {sections.map((s) => (
        <button
          key={s.id}
          type="button"
          onClick={() =>
            document
              .getElementById(s.id)
              ?.scrollIntoView({ block: "start", behavior: "smooth" })
          }
        >
          {s.label}
        </button>
      ))}
    </nav>
  );
}

/**
 * 启动磁贴。总览的 Claude 侧和 GPT 侧共用同一份。
 *
 * v0.7.0 从 `AccountBand.tsx` 里抽出来：总览分成 Claude / GPT 两个页签之后，
 * 两边都要用它。留在原地复制一份的话，两个磁贴的进度线、失败态、
 * 「代价常驻」这几件事迟早会走样，而它们恰恰是这个组件存在的理由。
 */

import type { ReactNode } from 'react';

import type { TaskState } from '../../lib/tasks';

export interface TileProps {
  icon: ReactNode;
  name: string;
  /** 常驻说明。**不要放进 title 属性** —— 代价必须一直看得见，见档案 §7。 */
  note: ReactNode;
  tone?: 'accent' | 'warn';
  task: TaskState;
  disabled?: boolean;
  onClick: () => void;
}

/**
 * 进度不再挂在贴外：正在启动时贴底出现一条 2px 进度线，`phase` 顶掉那行说明；
 * 失败则整贴变红并**留住错误原文**（toast 会自己消失，错误不能只靠它）。
 */
export default function Tile({ icon, name, note, tone, task, disabled, onClick }: TileProps) {
  const failed = !!task.error;
  const cls = ['tile', failed ? 'tile--danger' : tone ? `tile--${tone}` : '']
    .filter(Boolean)
    .join(' ');
  const pct = task.total > 0 ? (task.step / task.total) * 100 : undefined;

  return (
    <button type="button" className={cls} disabled={disabled} onClick={onClick}>
      <span className="tile-icon" aria-hidden="true">
        {icon}
      </span>
      <span className="tile-name">{name}</span>
      <span className="tile-note">
        {failed ? task.error : task.running ? task.phase || '启动中…' : note}
      </span>
      {(task.running || failed) && (
        <span
          className={[
            'tile-progress',
            failed ? 'tile-progress--danger' : '',
            task.running && pct === undefined ? 'tile-progress--indeterminate' : '',
          ]
            .filter(Boolean)
            .join(' ')}
          style={failed ? { width: '100%' } : pct !== undefined ? { width: `${pct}%` } : undefined}
        />
      )}
    </button>
  );
}

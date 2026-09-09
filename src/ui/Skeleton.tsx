interface Props {
  className?: string;
  /** 行数 > 1 时渲染成一叠，最后一行短一截，看起来像段落。 */
  lines?: number;
}

/** 加载占位。有它才能把「加载中」和「没有数据」在视觉上分开。 */
export default function Skeleton({ className = 'h-4 w-full', lines = 1 }: Props) {
  if (lines > 1) {
    return (
      <div className="flex flex-col gap-2" aria-hidden="true">
        {Array.from({ length: lines }, (_, i) => (
          <span
            key={i}
            className={`skeleton h-4 ${i === lines - 1 ? 'w-2/5' : 'w-full'}`}
          />
        ))}
      </div>
    );
  }
  return <span className={`skeleton ${className}`} aria-hidden="true" />;
}

/** 「酒馆」弹窗的「备份与回收站」分页：资产盘点、整份备份与恢复、回收站（第三轮从软件页的卡搬进弹窗）。 */
import Assets from "./Assets";
import TrashList from "./Trash";

export default function DataTab() {
  return (
    <div className="tv-data">
      <Assets />
      <TrashList />
    </div>
  );
}

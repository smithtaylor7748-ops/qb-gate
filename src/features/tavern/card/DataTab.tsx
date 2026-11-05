/** 软件页 SillyTavern 卡的「备份与回收站」分页：资产盘点、整份备份与恢复、回收站。 */
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

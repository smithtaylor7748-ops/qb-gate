/**
 * 软件与安装 —— 检测 / 安装 / 升级 / 版本回滚 / 外部副本清理。
 *
 * `qb-software` 那个类只做一件事：**把这一页的版式整体放大一档**。
 * 它是操作密度最高的一页，每一项都要读完说明再按，挤着看最费劲。
 * 具体放大了什么见 `workspace.css` 末尾那一段。
 */
import Environment from "../pages/Environment";

export default function Software() {
  return (
    <section className="qb-legacy-detail qb-software">
      <Environment />
    </section>
  );
}

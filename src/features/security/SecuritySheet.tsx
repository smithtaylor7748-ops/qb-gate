/**
 * 安全项的小窗。**全局只挂一份**，在 `Shell` 里。
 *
 * 总览的评分四格、门禁读数、纯净度爆红卡、GPT 那侧的「门禁范围」，
 * 点下去都开这一个 —— 谁都不用自己端一份状态，也不跳页。
 * 跟 `AccountDialogs` 的 `requestSwitch` 是同一个套路：
 * 谁想开就喊一声 `openSecurity(id)`。
 *
 * 窗里是**完整正文**（`objects.tsx` 的 `ObjectDetail`），不是精简版：
 * 小窗里少写一句话，就等于逼人再跳一次页去看全的。
 */
import { setSession, useSession } from "../../lib/store";
import { Modal } from "../../ui";
import { OBJECTS, ObjectAction, ObjectDetail, type ObjectId } from "./objects";

const KEY = "security.sheet";

/** 打开某一项安全对象的小窗。不在组件里也能调。 */
export function openSecurity(id: ObjectId): void {
  setSession<ObjectId | null>(KEY, id);
}

export default function SecuritySheet() {
  const [open, setOpen] = useSession<ObjectId | null>(KEY, null);
  const current = OBJECTS.find((o) => o.id === open);
  return (
    <Modal
      open={current !== undefined}
      onClose={() => setOpen(null)}
      size="wide"
      title={
        current ? (
          <span className="qb-modal-title">
            {current.name}
            <small>{current.sub}</small>
          </span>
        ) : (
          ""
        )
      }
      footer={current ? <ObjectAction id={current.id} /> : undefined}
    >
      {current && <ObjectDetail id={current.id} />}
    </Modal>
  );
}

/**
 * 页面里反复出现的两条提醒，各写一处：
 *
 *   - `AddressNotice`：地址只看格式、填本人真实地址。**使用者要求标红**。
 *   - `ScopeNotice`：「国内」指两家都提供服务的国家和地区。
 *
 * 弹窗、答疑页、三条路线里用的都是这两个组件，改文案只改这里。
 */
import { Info, ShieldAlert } from "lucide-react";
import { ExternalLink } from "../../ui";
import {
  ADDRESS_GENERATOR_OWN_WORDS,
  ADDRESS_GENERATOR_URL,
  CLAUDE_REGIONS_URL,
  OPENAI_REGIONS_URL,
} from "./data";

interface AddressProps {
  /** 步骤里用的短版：只留结论，不重复整段。 */
  compact?: boolean;
}

export function AddressNotice({ compact = false }: AddressProps) {
  if (compact)
    return (
      <div className="qb-sub-alert qb-sub-alert--danger" role="note">
        <ShieldAlert size={18} aria-hidden="true" />
        <div className="qb-sub-alert-body">
          <p>
            <strong>特别注意：</strong>
            免税州地址与
            <ExternalLink href={ADDRESS_GENERATOR_URL}>地址生成器</ExternalLink>
            生成的地址<strong>只是格式参考</strong>，看「街道 / 城市 / 州 /
            邮编」各填在哪一栏；
            <strong>填表时请填写你本人的真实地址。</strong>
          </p>
        </div>
      </div>
    );

  return (
    <div className="qb-sub-alert qb-sub-alert--danger" role="note">
      <ShieldAlert size={20} aria-hidden="true" />
      <div className="qb-sub-alert-body">
        <p className="qb-sub-alert-title">
          特别注意 · 地址只看格式，填的必须是你本人的真实地址
        </p>
        <p>
          本页出现的美国免税州街道、城市、州缩写和 ZIP Code，以及
          <ExternalLink href={ADDRESS_GENERATOR_URL}>
            免税州地址生成器
          </ExternalLink>
          生成出来的地址，都<strong>只是格式参考</strong>——
          拿它看一个美国地址长什么样、「街道 / 城市 / 州 /
          邮编」各填在哪一栏、怎么对齐。
        </p>
        <p>
          <strong>
            注册账户、登记付款资料、在收银台填账单地址时，请填写你本人的真实地址与真实信息。
          </strong>
          那个网站自己也写着：「{ADDRESS_GENERATOR_OWN_WORDS}
          」把参考格式当成自己的资料提交，由此引发的风控审查、拒付、税务与法律后果，由填报者本人承担。
        </p>
      </div>
    </div>
  );
}

export function ScopeNotice() {
  return (
    <div className="qb-sub-alert qb-sub-alert--accent" role="note">
      <Info size={20} aria-hidden="true" />
      <div className="qb-sub-alert-body">
        <p className="qb-sub-alert-title">本页说的「国内」「本地」是什么意思</p>
        <p>
          指 Anthropic（Claude）与 OpenAI（ChatGPT）
          <strong>两家都正式提供服务</strong>
          的国家和地区，以两家官方名单为准：
          <ExternalLink href={CLAUDE_REGIONS_URL}>
            Claude 支持的国家与地区
          </ExternalLink>
          <span aria-hidden="true"> · </span>
          <ExternalLink href={OPENAI_REGIONS_URL}>
            ChatGPT 支持的国家与地区
          </ExternalLink>
          。
        </p>
        <p>
          任何一家不提供服务的国家或地区，都<strong>不在</strong>
          本页「国内」的范围内；本页也不讨论从这些地方怎么注册、付款或使用。
        </p>
      </div>
    </div>
  );
}

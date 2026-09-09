import { useId, useState, type ReactNode } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { AlertCircle, FolderOpen } from 'lucide-react';
import Button from './Button';

interface FieldProps {
  label: ReactNode;
  hint?: ReactNode;
  error?: string;
  children: (props: { id: string; 'aria-invalid': boolean | undefined }) => ReactNode;
  className?: string;
}

/**
 * label + 控件 + 说明 + 错误，`htmlFor` / `id` 正确关联。
 *
 * 旧代码用 `<label><span class="k">标识 id</span><input/></label>` 包起来 ——
 * 嵌套关联在多数读屏器上能用，但一旦控件不是直接子元素就断，而且
 * `.k` 是给指标块用的类名，语义上不是 label。
 */
export function Field({ label, hint, error, children, className = '' }: FieldProps) {
  const id = useId();
  return (
    <div className={className}>
      <label className="field-label" htmlFor={id}>
        {label}
      </label>
      {children({ id, 'aria-invalid': error ? true : undefined })}
      {error ? (
        <p className="field-error">
          <AlertCircle size={11} aria-hidden="true" />
          {error}
        </p>
      ) : hint ? (
        <p className="field-hint">{hint}</p>
      ) : null}
    </div>
  );
}

interface TextFieldProps {
  label: ReactNode;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  hint?: ReactNode;
  error?: string;
  type?: 'text' | 'password';
  disabled?: boolean;
  className?: string;
}

export function TextField({
  label,
  value,
  onChange,
  placeholder,
  hint,
  error,
  type = 'text',
  disabled,
  className,
}: TextFieldProps) {
  return (
    <Field label={label} hint={hint} error={error} className={className}>
      {(p) => (
        <input
          {...p}
          className="input"
          type={type}
          value={value}
          placeholder={placeholder}
          disabled={disabled}
          spellCheck={false}
          autoComplete={type === 'password' ? 'off' : undefined}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </Field>
  );
}

interface PathFieldProps {
  label: ReactNode;
  value: string;
  onChange: (v: string) => void;
  hint?: ReactNode;
  /** 选目录还是选文件。启动脚本是文件，其余两个是目录。 */
  kind?: 'directory' | 'file';
  disabled?: boolean;
}

/**
 * 路径输入 + 目录选择器。
 *
 * `@tauri-apps/plugin-dialog` 一直躺在 package.json 里从没被用过，
 * 而酒馆面板的三个路径要用户手打绝对路径 —— 打错一个字符就是启动失败，
 * 报错还在很后面才出现。
 */
export function PathField({
  label,
  value,
  onChange,
  hint,
  kind = 'directory',
  disabled,
}: PathFieldProps) {
  const [picking, setPicking] = useState(false);

  async function pick() {
    setPicking(true);
    try {
      const picked = await openDialog({
        directory: kind === 'directory',
        multiple: false,
        defaultPath: value || undefined,
      });
      if (typeof picked === 'string') onChange(picked);
    } catch {
      // 用户取消或者对话框起不来，保持原值即可，不打扰。
    } finally {
      setPicking(false);
    }
  }

  return (
    <Field label={label} hint={hint}>
      {(p) => (
        <div className="flex gap-2">
          <input
            {...p}
            className="input font-mono"
            type="text"
            value={value}
            disabled={disabled}
            spellCheck={false}
            onChange={(e) => onChange(e.target.value)}
          />
          <Button
            icon={<FolderOpen size={13} aria-hidden="true" />}
            onClick={pick}
            loading={picking}
            disabled={disabled}
          >
            浏览
          </Button>
        </div>
      )}
    </Field>
  );
}

interface PortFieldProps {
  label: ReactNode;
  value: number;
  onChange: (v: number) => void;
  hint?: ReactNode;
  disabled?: boolean;
}

/**
 * 端口输入。
 *
 * 旧代码用 `type="text"` + `Number(e.target.value) || 0`：输入字母**静默变成 0**，
 * 用户看着自己打的字消失，也不知道为什么。这里用 number 并校验范围。
 */
export function PortField({ label, value, onChange, hint, disabled }: PortFieldProps) {
  const [raw, setRaw] = useState<string | null>(null);
  const shown = raw ?? String(value);
  const n = Number(shown);
  const invalid = shown !== '' && (!Number.isInteger(n) || n < 1 || n > 65535);

  return (
    <Field
      label={label}
      hint={hint}
      error={invalid ? '端口要是 1–65535 之间的整数' : undefined}
    >
      {(p) => (
        <input
          {...p}
          className="input"
          type="number"
          min={1}
          max={65535}
          step={1}
          value={shown}
          disabled={disabled}
          onChange={(e) => {
            setRaw(e.target.value);
            const v = Number(e.target.value);
            if (Number.isInteger(v) && v >= 1 && v <= 65535) onChange(v);
          }}
          onBlur={() => setRaw(null)}
        />
      )}
    </Field>
  );
}

interface CheckboxProps {
  checked: boolean;
  onChange: (v: boolean) => void;
  children: ReactNode;
  disabled?: boolean;
}

export function Checkbox({ checked, onChange, children, disabled }: CheckboxProps) {
  return (
    <label className="checkbox">
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span>{children}</span>
    </label>
  );
}

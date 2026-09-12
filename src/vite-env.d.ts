/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** `1` = 走 src/lib/demo.ts 的演示数据，只在 `npm run demo` 下设。 */
  readonly VITE_DEMO?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

import { call } from "./ipc";
import type { QualityCheckRequest } from "./generated/QualityCheckRequest";
import type { QualityCheckResult } from "./generated/QualityCheckResult";

export type { QualityCheckRequest, QualityCheckResult };

/** Only an explicit UI action starts a model request; target credentials stay in Rust. */
export const qualityApi = {
  run: (request: QualityCheckRequest) =>
    call<QualityCheckResult>("quality_check_run", { request }),
  cancel: (requestId: string) =>
    call<boolean>("quality_check_cancel", { requestId }),
};

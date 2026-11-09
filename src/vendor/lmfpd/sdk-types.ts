// SPDX-License-Identifier: MIT
// Copyright (c) 2026 xqy2006. See LICENSE and README.md in this directory.
export interface Output { text: string; expected_count: number }

export interface Challenge { id: string; expected_count: number; prompt: string }

export interface AnalysisResult {
  model: string
  display_name: string
  family: string
  family_name: string
  score: number
  probability: number | null
}

export interface Diagnostic {
  index: number
  parsed_numbers: number
  minimum_numbers: number
  accepted: boolean
  raw_out_of_range_count: number
}

export interface Analysis {
  decision: 'scored' | 'partial' | 'unscorable' | 'not_model'
  prediction: string
  prediction_name: string
  family_prediction: string
  family_prediction_name: string
  /** Candidates by ranking score, best first. Empty unless the decision is `scored` or `partial`. */
  results: AnalysisResult[]
  ranking_score: number | null
  /** Closed-set probability of the top candidate within the reference bank. Null without calibration or three replies. */
  probability: number | null
  probability_status: 'unavailable' | 'reference_calibrated'
  probability_scope: string | null
  calibration: { method: string; run: string; tau: number; sha256: string | null } | null
  used_outputs: number
  diagnostics: Diagnostic[]
  sequence: { autocorrelation: number; small_step_share: number; model_like: boolean } | null
  method: string
  model_version: { base_sha256: string }
  evidence: { insufficient: boolean; label: string; reason: string; threshold: null; method: string }
}

// SPDX-License-Identifier: MIT
// Copyright (c) 2026 xqy2006. See LICENSE and README.md in this directory.
// Pure SDK helpers extracted without their CLI/network orchestration.
import type { Analysis } from './types';

export const minimumNumbers = (expected: number) => Math.max(80, Math.ceil(expected * 0.55))

export function englishAnalysis(analysis: Analysis): Analysis {
  let label: string, reason: string
  if (analysis.decision === 'unscorable') {
    label = 'Not enough valid samples'
    reason = `Received ${analysis.used_outputs}/3 valid samples. Three valid samples are required.`
  } else if (analysis.decision === 'not_model') {
    label = 'Not like a language model reply'
    reason = 'Consecutive numbers change differently from the language-model replies in the reference bank. Program-generated, sorted or shuffled numbers often do this. No ranking was produced.'
  } else if (analysis.decision === 'partial') {
    label = 'Partial sample ranking'
    reason = `Ranked ${analysis.used_outputs} valid sample${analysis.used_outputs === 1 ? '' : 's'}. Confidence requires three valid samples.`
  } else if (analysis.probability_status === 'reference_calibrated') {
    label = 'Calibrated ranking'
    reason = 'Confidence is a closed-set probability within the reference bank.'
  } else {
    label = 'Uncalibrated ranking'
    reason = 'The confidence calibration does not match this detector. Confidence is unavailable.'
  }
  return {
    ...analysis, prediction_name: analysis.decision === 'unscorable' ? 'Not scored'
      : analysis.decision === 'not_model' ? 'Not a model reply' : analysis.prediction_name,
    evidence: { ...analysis.evidence, label, reason },
  }
}


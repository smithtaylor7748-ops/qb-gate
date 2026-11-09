// SPDX-License-Identifier: MIT
// Copyright (c) 2026 xqy2006. See LICENSE and README.md in this directory.
import type { Challenge } from './sdk-types';
export interface ChallengeRandom { getRandomValues(array: Uint32Array): Uint32Array; randomUUID(): string; }
export function generateChallenges(count?: number, usedLengths?: number[], random?: ChallengeRandom): Challenge[];

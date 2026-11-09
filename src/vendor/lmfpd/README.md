# Offline lmfpd subset used by QB Gate

This directory contains MIT-licensed code from `Ikaleio/lm-detector`, commit
`1e58ed83c1cde17f88cba89560ad4d083042ae92` (`lmfpd-sdk` 0.2.0), and metadata
from `lmfpd-data` 0.0.1793708131735. Copyright (c) 2026 xqy2006. The unmodified
MIT license is in `LICENSE`; the data license is also distributed in
`public/quality-reference/lmfpd-0.2.0/LICENSE`.

`provenance.json` records exact npm integrity values, archive SHA-256 digests,
upstream Git blob IDs, original source hashes and reference-data hashes.
Both npm tarballs were checked against their published SHA-512 integrity;
each upstream source was checked against the fixed commit's Git blob ID.

The reference binary is decompressed once during vendoring and is bundled with
the app. Its SHA-256 is compiled into `src/lib/qualityFingerprint.ts`, verified
before decoding, and independent of any fetched manifest. There is no external
download, latest-version resolution or automatic data update at runtime.

## Adaptations

- `shared-detector.ts`, `positional.ts`, `detector-codec.ts` and
  `fingerprint-core.js` preserve the upstream numerical algorithm. Only an MIT
  notice was prepended.
- `analysis.ts` preserves the SDK's `analyzeOutputs`, scoring and canonical
  result conversion; imports point at this local subset.
- `sample-shape.ts` contains the original pure `minimumNumbers` and
  `englishAnalysis` helpers. Their network/CLI orchestration was excluded.
- `types.ts` and `sdk-types.ts` retain the offline types used by those modules;
  unused transport configuration and its imports were excluded. Declarations
  for the two JavaScript modules were added for this TypeScript application.
- `challenge-browser.js` preserves all original prompts and rejection-sampling
  rules. An optional local random source is threaded through its helpers, so
  QB can reproduce prompts from a run seed without changing global crypto.
- No Node filesystem, process, proxy, cache, request or zstd implementation is
  included in the runtime subset. In particular, the SDK's bundled `fzstd`
  dependency is not copied. QB uses its own existing request paths separately.

QB's wrapper requires three full-length answers for its reference-class
comparison, while preserving the raw SDK analysis and its original 55% sample
acceptance threshold. Neither library ranking nor QB comparison authenticates
an upstream model. The bank explicitly combines `gpt-6-astra/6.1-sol`; those
members cannot be distinguished. Unknown model names stay unknown.

`qualityFingerprint.test.ts` checks saved results produced by the original npm
SDK on the same synthetic inputs, including candidate scores, calibrated
probabilities and predictions. These are algorithm conformance checks, not
measurements of live model quality.

To verify shipped assets and license copies, run:

```powershell
node scripts/check-quality-reference.mjs
```

Data updates must be deliberate: verify the new package's integrity and license,
regenerate the local binary/metadata, update the compiled version/hash bindings,
and repeat conformance tests. Do not overwrite this bank from a runtime URL.

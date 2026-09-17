# Contributing

QB Gate is a Windows desktop project dual-licensed under AGPL-3.0-only and a commercial license (see LICENSE and LICENSE-COMMERCIAL.md). Contributions must be compatible with that arrangement: by submitting code you confirm you have the right to license it and you grant the maintainers the right to use, modify and sublicense it under **both** AGPL-3.0-only and a commercial license. Code you cannot license on those terms — including third-party code under a copyleft or non-commercial licence — cannot be accepted. Record copied or adapted material and its original notices in ATTRIBUTION.md; do not copy code without a verified license.

Read CLAUDE.md and docs/ARCHITECTURE.zh-CN.md before changing account, process, ACL, migration, or installation behavior. Keep executable discovery in install/inventory.rs and gate policy in gate/judge.rs.

## Development

Use Node.js 24 and Rust stable with MSVC Windows build tools. Install dependencies with `npm ci`. Commit both lockfiles. Run frontend tests/build, Rust tests, formatting, generated-type checks, release source checks, and UI regression. Generate new IPC boundary types with `npm run types:generate`.

Unit tests must use temporary directories and fixtures. Never write the real runtime directory, modify installed software ACLs, terminate real user processes, log into a real account, or call paid APIs from tests. UI tests run in demo mode with external requests blocked.

## Issues and pull requests

Describe the trigger, observed behavior, expected behavior, version, and validation. Include a minimal redacted example when relevant. Do not upload credentials, OAuth files, runtime databases, snapshots, local machine paths, private repositories, or raw diagnostic responses containing secrets.

A pull request should keep official and relay identities separate, preserve user settings it does not own, report partial failures accurately, and include meaningful regression coverage for state or recovery changes. The UI must remain usable with a keyboard and at the minimum supported window size.

## Releases

The package.json, Cargo.toml, Cargo.lock and tauri.conf.json versions must agree. A release tag is `v<version>` and must match those files. Update migration notes, regression results, dependency notices and screenshots where relevant. CI creates the installer from the tagged source and attaches checksums. Publication is a separate action from local build and verification.

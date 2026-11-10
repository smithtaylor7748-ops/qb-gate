// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = qb_platform::macos::handle_helper_args() {
        std::process::exit(code);
    }
    if let Some(code) = qb_iplock::gate::hook::handle_helper_args() {
        std::process::exit(code);
    }
    qb_gate_lib::run()
}

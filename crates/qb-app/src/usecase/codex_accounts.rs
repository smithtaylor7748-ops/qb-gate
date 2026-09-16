//! Explicit Codex desktop account switching, isolated from relay scheduling.
use crate::error::Result;
use qb_accounts::codex;
use qb_install::install::codex_desktop;

pub fn switch(id: &str) -> Result<()> {
    let root = codex::root();
    codex::directory(&root, id)?; // Validate before stopping any task.
    codex_desktop::close()?;
    codex::select(&root, id)
}

pub fn launch(id: &str) -> Result<()> {
    let root = codex::root();
    let dir = codex::directory(&root, id)?;
    codex_desktop::close()?;
    let process = codex_desktop::launch(&dir.join("home"), &dir.join("desktop"))?;
    codex::mark_launched(&root, id, process.pid, &process.started)
}

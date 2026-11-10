//! Pure executable-path check shared by storage guards. Never inspects user data.
use std::path::Path;

pub fn running_in_a_test_binary(exe: Option<&Path>) -> bool {
    let Some(exe) = exe else { return false };
    let in_deps = exe
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|d| d == "deps");
    let hashed = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.rsplit_once('-'))
        .is_some_and(|(_, h)| h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit()));
    in_deps && hashed
}

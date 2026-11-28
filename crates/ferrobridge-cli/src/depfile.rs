//! Make-syntax dependency files (`generate --depfile`): which files the generated bridge depends
//! on, so a build system (CMake with Ninja or Make) regenerates it when an indirectly included header
//! changes (ADR-0013).

use std::path::{Path, PathBuf};

/// `target: dep1 dep2 …`, one dependency per line, with spaces, `#` and `$` escaped the way GCC
/// writes them and Ninja and Make read them.
pub(crate) fn render(target: &Path, dependencies: &[PathBuf]) -> String {
    let mut text = escape(target);
    text.push(':');
    for dependency in dependencies {
        text.push_str(" \\\n  ");
        text.push_str(&escape(dependency));
    }
    text.push('\n');
    text
}

fn escape(path: &Path) -> String {
    let mut text = String::new();
    for c in path.to_string_lossy().chars() {
        match c {
            ' ' => text.push_str("\\ "),
            '#' => text.push_str("\\#"),
            '$' => text.push_str("$$"),
            c => text.push(c),
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_dependency_per_line() {
        let text = render(
            Path::new("out/bridge.rs"),
            &[PathBuf::from("/src/a.h"), PathBuf::from("/src/b.h")],
        );
        assert_eq!(text, "out/bridge.rs: \\\n  /src/a.h \\\n  /src/b.h\n");
    }

    #[test]
    fn spaces_hashes_and_dollars_are_escaped() {
        let text = render(
            Path::new("my out/bridge.rs"),
            &[PathBuf::from("/My Docs/$lib/#1.h")],
        );
        assert_eq!(text, "my\\ out/bridge.rs: \\\n  /My\\ Docs/$$lib/\\#1.h\n");
    }
}

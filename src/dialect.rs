//! File-name to dialect mapping.
//!
//! The table is Bazel's, reproduced from the behaviour of `bazel` 9.2.0 and
//! cross-checked against starpls `crates/starpls/src/document.rs`.

use std::path::Path;

/// Grammar-level variations. This selects how a file is *parsed*, not which
/// globals are in scope — that is the consumer's problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Dialect {
    /// Plain Starlark per the spec: `load()` takes a filesystem path.
    #[default]
    Standard,
    /// Bazel's dialect: `load()` takes a label, type syntax is available.
    Bazel,
    /// Starlark Configuration Language. Predeclared environment is exactly
    /// `{visibility, struct}`; `load()` may only reference other `.scl` files.
    Scl,
}

/// Which Bazel file a path denotes. Carried alongside [`Dialect`] because the
/// consumer needs it to pick a global environment, and only the file name knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    Build,
    Bzl,
    Module,
    Workspace,
    Repo,
    Vendor,
    Cquery,
    Prelude,
    Scl,
}

impl Dialect {
    /// Whether Bazel 9 type-annotation syntax is accepted.
    ///
    /// `--experimental_starlark_type_syntax` defaults to true in Bazel 9.0.0
    /// through HEAD, so this is on wherever the Bazel dialect is.
    #[must_use]
    pub fn allows_type_syntax(self) -> bool {
        matches!(self, Self::Bazel | Self::Scl)
    }

    /// Whether `cast` and `isinstance` lex as keywords rather than identifiers.
    /// Bazel gates these on the same flag as type syntax.
    #[must_use]
    pub fn has_type_keywords(self) -> bool {
        self.allows_type_syntax()
    }
}

/// Classify a path. Returns `None` for files that are not Starlark at all.
///
/// `workspace_root` resolves the one entry that is path- rather than
/// name-addressed: `tools/build_rules/prelude_bazel`.
#[must_use]
pub fn classify(path: &Path, workspace_root: Option<&Path>) -> Option<(Dialect, FileKind)> {
    let name = path.file_name()?.to_str()?;

    let by_name = match name {
        "BUILD" | "BUILD.bazel" => Some(FileKind::Build),
        "MODULE.bazel" => Some(FileKind::Module),
        "REPO.bazel" => Some(FileKind::Repo),
        "VENDOR.bazel" => Some(FileKind::Vendor),
        "WORKSPACE" | "WORKSPACE.bazel" | "WORKSPACE.bzlmod" => Some(FileKind::Workspace),
        _ => None,
    };
    if let Some(kind) = by_name {
        return Some((Dialect::Bazel, kind));
    }

    for (suffix, kind) in [
        (".MODULE.bazel", FileKind::Module),
        (".BUILD.bazel", FileKind::Build),
        (".BUILD", FileKind::Build),
        (".cquery", FileKind::Cquery),
        (".query.bzl", FileKind::Cquery),
    ] {
        if name.ends_with(suffix) {
            return Some((Dialect::Bazel, kind));
        }
    }

    match path.extension().and_then(|e| e.to_str()) {
        Some("bzl") => Some((Dialect::Bazel, FileKind::Bzl)),
        Some("scl") => Some((Dialect::Scl, FileKind::Scl)),
        _ => {
            let is_prelude = workspace_root
                .is_some_and(|root| path == root.join("tools/build_rules/prelude_bazel"));
            is_prelude.then_some((Dialect::Bazel, FileKind::Prelude))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(p: &str) -> Option<(Dialect, FileKind)> {
        classify(Path::new(p), Some(Path::new("/ws")))
    }

    #[test]
    fn recognises_bazel_files() {
        assert_eq!(kind("a/BUILD"), Some((Dialect::Bazel, FileKind::Build)));
        assert_eq!(
            kind("a/BUILD.bazel"),
            Some((Dialect::Bazel, FileKind::Build))
        );
        assert_eq!(kind("a/defs.bzl"), Some((Dialect::Bazel, FileKind::Bzl)));
        assert_eq!(
            kind("MODULE.bazel"),
            Some((Dialect::Bazel, FileKind::Module))
        );
        assert_eq!(
            kind("x.MODULE.bazel"),
            Some((Dialect::Bazel, FileKind::Module))
        );
        assert_eq!(kind("zlib.BUILD"), Some((Dialect::Bazel, FileKind::Build)));
        assert_eq!(
            kind("WORKSPACE.bzlmod"),
            Some((Dialect::Bazel, FileKind::Workspace))
        );
    }

    #[test]
    fn recognises_scl() {
        assert_eq!(kind("PROJECT.scl"), Some((Dialect::Scl, FileKind::Scl)));
        assert!(Dialect::Scl.allows_type_syntax());
    }

    #[test]
    fn prelude_is_path_addressed() {
        assert_eq!(
            kind("/ws/tools/build_rules/prelude_bazel"),
            Some((Dialect::Bazel, FileKind::Prelude))
        );
        assert_eq!(kind("/other/tools/build_rules/prelude_bazel"), None);
    }

    #[test]
    fn ignores_non_starlark() {
        assert_eq!(kind("main.rs"), None);
        assert_eq!(kind(".bazelrc"), None);
    }
}

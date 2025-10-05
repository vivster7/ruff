use std::path::PathBuf;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_stdlib::sys::is_known_standard_library;

use crate::checkers::ast::Checker;
use crate::Violation;

/// ## What it does
/// Checks for imports of symbols from modules, rather than importing modules directly.
///
/// ## Why is this bad?
/// When using lazy imports with `importlib.util.LazyLoader`, only module imports
/// are supported. Importing symbols directly prevents lazy loading and can impact
/// application startup time.
///
/// This pattern also encourages better encapsulation and makes dependencies more explicit.
///
/// ## Example
/// ```python
/// # Bad - importing symbol
/// from foo.bar import MyClass
///
/// # Good - importing module
/// from foo import bar
/// bar.MyClass
///
/// # Also good
/// import foo.bar
/// foo.bar.MyClass
/// ```
///
/// ## Options
/// - `lint.ruff.non-module-import-check-first-party`
/// - `lint.ruff.non-module-import-check-stdlib`
/// - `lint.ruff.non-module-import-check-third-party`
/// - `lint.ruff.non-module-import-allow-modules`
/// - `lint.ruff.non-module-import-third-party-module-paths`
#[derive(ViolationMetadata)]
pub(crate) struct NonModuleImport {
    module: String,
    name: String,
}

impl Violation for NonModuleImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonModuleImport { module, name } = self;
        format!("`{name}` imported from `{module}` is not a module")
    }
}

/// Default modules to allow importing symbols from
const DEFAULT_ALLOWED_MODULES: &[&str] = &[
    "typing",
    "__future__",
    "typing_extensions",
    "collections.abc",
];

/// Check if the import is inside a TYPE_CHECKING block
fn is_in_type_checking_block(checker: &Checker) -> bool {
    checker.semantic().in_type_checking_block()
}

/// Check if a dotted name is a module by checking the filesystem
fn is_module_on_filesystem(
    full_import_path: &str,
    src_dirs: &[PathBuf],
) -> bool {
    for src_dir in src_dirs {
        // Convert "a.b.c" to "a/b/c"
        let relative_path: PathBuf = full_import_path.split('.').collect();
        let candidate = src_dir.join(&relative_path);

        // Is it a package directory (with __init__.py)?
        if candidate.is_dir() && candidate.join("__init__.py").exists() {
            return true;
        }

        // Is it a module file (.py)?
        if candidate.with_extension("py").is_file() {
            return true;
        }

        // Is it a stub file (.pyi)?
        if candidate.with_extension("pyi").is_file() {
            return true;
        }
    }

    false
}

/// RUF066
pub(crate) fn non_module_import(
    checker: &mut Checker,
    import_from: &ast::StmtImportFrom,
) {
    // Skip if inside TYPE_CHECKING block
    if is_in_type_checking_block(checker) {
        return;
    }

    let module_path = import_from.module.as_ref().map(|m| m.as_str()).unwrap_or("");

    // Skip if module is in the default allow-list
    if DEFAULT_ALLOWED_MODULES.contains(&module_path) {
        return;
    }

    // Skip if module is in the user-configured allow-list
    let settings = &checker.settings().ruff;
    if settings.non_module_import_allow_modules.iter().any(|m| m == module_path) {
        return;
    }

    // Check first segment of module path to determine import type
    let module_base = module_path.split('.').next().unwrap_or("");
    let is_stdlib = !module_base.is_empty()
        && is_known_standard_library(checker.target_version().minor, module_base);

    // Skip based on import type and settings
    if is_stdlib && !settings.non_module_import_check_stdlib {
        return;
    }

    // Get source directories from settings, plus the directory of the current file
    let mut src_dirs = checker.settings().src.clone();
    if let Some(parent) = checker.path().parent() {
        src_dirs.push(parent.to_path_buf());
    }

    // Determine if this is first-party or third-party based on filesystem
    // If the module base is found in src_dirs, it's first-party
    let is_first_party = is_module_on_filesystem(module_base, &src_dirs);

    // Skip based on first-party/third-party settings
    if is_first_party && !settings.non_module_import_check_first_party {
        return;
    }
    if !is_first_party && !is_stdlib && !settings.non_module_import_check_third_party {
        return;
    }

    // Build search paths: src_dirs for first-party, third-party paths for third-party
    let search_paths = if is_first_party || is_stdlib {
        src_dirs.clone()
    } else {
        // For third-party, use configured paths
        let mut paths = settings.non_module_import_third_party_module_paths.clone();
        // Also include src_dirs in case they have vendored dependencies
        paths.extend(src_dirs);
        paths
    };

    for alias in &import_from.names {
        let imported_name = alias.name.as_str();

        // Skip wildcard imports
        if imported_name == "*" {
            continue;
        }

        // Build full import path: "module.name"
        let full_path = if module_path.is_empty() {
            imported_name.to_string()
        } else {
            format!("{}.{}", module_path, imported_name)
        };

        // Check if it's a module on the filesystem
        if !is_module_on_filesystem(&full_path, &search_paths) {
            // It's not a module, report violation
            checker.report_diagnostic(
                NonModuleImport {
                    module: module_path.to_string(),
                    name: imported_name.to_string(),
                },
                alias.range,
            );
        }
    }
}

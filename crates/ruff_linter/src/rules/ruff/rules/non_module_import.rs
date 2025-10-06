use std::path::PathBuf;

use anyhow::Result;

use ruff_diagnostics::Edit;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_semantic::{
    Alias, Binding, Imported, MemberNameImport, ModuleNameImport, NameImport, Scope, SemanticModel,
};
use ruff_python_stdlib::builtins::is_python_builtin;
use ruff_python_stdlib::keyword::is_keyword;
use ruff_python_stdlib::sys::is_known_standard_library;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::remove_unused_imports;
use crate::{Fix, FixAvailability, Violation};

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
/// ## Fix safety
/// The fix is marked as unsafe because:
/// - Import aliases are lost (e.g., `from X import Y as Z` becomes `X.Y`, not `Z`)
/// - The import structure is significantly changed
/// - In rare cases with complex shadowing, the chosen alias might be suboptimal
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonModuleImport { module, name } = self;
        format!("`{name}` imported from `{module}` is not a module")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Convert to module import and update references".to_string())
    }
}

/// Default modules to allow importing symbols from
const DEFAULT_ALLOWED_MODULES: &[&str] = &[
    "typing",
    "__future__",
    "typing_extensions",
    "collections.abc",
];

/// Known stdlib submodules that are importable
/// These are not in the top-level stdlib list but are valid module imports
const KNOWN_STDLIB_SUBMODULES: &[&str] = &[
    "collections.abc",
    "os.path",
    "importlib.metadata",
    "importlib.resources",
    "importlib.util",
    "importlib.abc",
    "email.mime",
    "email.mime.text",
    "email.mime.multipart",
    "email.mime.image",
    "email.mime.audio",
    "email.mime.base",
    "email.mime.message",
    "html.parser",
    "html.entities",
    "http.client",
    "http.server",
    "http.cookies",
    "http.cookiejar",
    "urllib.parse",
    "urllib.request",
    "urllib.response",
    "urllib.error",
    "urllib.robotparser",
    "xml.etree",
    "xml.etree.ElementTree",
    "xml.dom",
    "xml.dom.minidom",
    "xml.sax",
    "xml.parsers.expat",
];

/// Check if a dotted name is a module by checking the filesystem
fn is_module_on_filesystem(full_import_path: &str, src_dirs: &[PathBuf]) -> bool {
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

/// Check if a name is available (not shadowed) in a given scope and all reference scopes
/// or if it's already bound to an import of the same module (which we can reuse)
///
/// Returns (is_available, is_reusing_existing_import)
fn is_name_available(
    name: &str,
    module_to_import: &str,
    binding: &Binding,
    import_scope: &Scope,
    semantic: &SemanticModel,
    checker: &Checker,
) -> (bool, bool) {
    // Check keywords
    if is_keyword(name) {
        return (false, false);
    }

    // Check builtins
    if is_python_builtin(
        name,
        checker.target_version().minor,
        checker.source_type.is_ipynb(),
    ) {
        return (false, false);
    }

    // Check if available in the import's scope
    if !semantic.is_available_in_scope(name, binding.scope) {
        // Name is taken - check if it's bound to the same module we want to import
        if let Some(existing_binding_id) = import_scope.get(name) {
            let existing_binding = semantic.binding(existing_binding_id);
            // Check if it's an import of the exact module we want
            if let Some(import) = existing_binding.as_any_import() {
                let qual_name = import.qualified_name();
                // Compare as strings - for "import requests", qualified_name is ["requests"]
                // which displays as "requests"
                let qual_str = qual_name.segments().join(".");
                if qual_str == module_to_import {
                    // It's already importing the same module - we can reuse it!
                    // Return (true, true) to indicate the name is available AND we're reusing
                    return (true, true);
                }
            }
        }
        return (false, false);
    }

    // Check if available in all reference scopes
    for ref_id in binding.references() {
        let reference = semantic.reference(ref_id);
        if !semantic.is_available_in_scope(name, reference.scope_id()) {
            return (false, false);
        }
    }

    (true, false)
}

/// Extract parent context for unique naming
/// "requests.adapters" -> Some("requests_adapters_mod")
/// "foo.bar.baz" -> Some("bar_baz_mod")
fn extract_parent_context(module_path: &str) -> Option<String> {
    let parts: Vec<&str> = module_path.split('.').collect();
    if parts.len() > 1 {
        // Take last 2 parts for context
        let context_parts = &parts[parts.len().saturating_sub(2)..];
        Some(format!("{}_mod", context_parts.join("_")))
    } else {
        None
    }
}

/// Find a unique name for the import that doesn't shadow anything
/// Returns (import_name, optional_alias, reusing_existing_import)
///
/// Tries in order:
/// 1. Base name without alias
/// 2. Base name with "_mod" suffix
/// 3. Base name with parent context (e.g., "requests_adapters_mod")
/// 4. Base name with incrementing number (e.g., "base_mod2", "base_mod3", ...)
///
/// The third return value indicates whether we found an existing import of the same module
/// that we can reuse (avoiding the need to add a duplicate import statement).
fn find_unique_import_name(
    base_name: &str,
    module_path: &str,
    binding: &Binding,
    import_scope: &Scope,
    checker: &Checker,
) -> (String, Option<String>, bool) {
    let semantic = checker.semantic();

    // Strategy 1: Try base name without alias
    let (is_available, is_reusing) = is_name_available(
        base_name,
        module_path,
        binding,
        import_scope,
        semantic,
        checker,
    );
    if is_available {
        return (base_name.to_string(), None, is_reusing);
    }

    // Strategy 2: Try with _mod suffix
    let with_mod = format!("{}_mod", base_name);
    let (is_available, is_reusing) = is_name_available(
        &with_mod,
        module_path,
        binding,
        import_scope,
        semantic,
        checker,
    );
    if is_available {
        return (base_name.to_string(), Some(with_mod), is_reusing);
    }

    // Strategy 3: Try with parent context
    if let Some(parent_name) = extract_parent_context(module_path) {
        let (is_available, is_reusing) = is_name_available(
            &parent_name,
            module_path,
            binding,
            import_scope,
            semantic,
            checker,
        );
        if is_available {
            return (base_name.to_string(), Some(parent_name), is_reusing);
        }
    }

    // Strategy 4: Try with incrementing numbers
    for i in 2..100 {
        let numbered = format!("{}_mod{}", base_name, i);
        let (is_available, is_reusing) = is_name_available(
            &numbered,
            module_path,
            binding,
            import_scope,
            semantic,
            checker,
        );
        if is_available {
            return (base_name.to_string(), Some(numbered), is_reusing);
        }
    }

    // Fallback: Use a very unique name (should rarely happen)
    // Not reusing since this is a fallback
    (
        base_name.to_string(),
        Some(format!("{}_mod_imported", base_name)),
        false,
    )
}

/// Determine the import strategy for a given module path
///
/// Returns (NameImport to add, reference_prefix to use)
///
/// Examples:
/// - "requests" with level 0 -> (import requests, "requests")
/// - "requests.adapters" with level 0 -> (from requests import adapters, "adapters")
/// - "foo" with level 1 -> (from . import foo, "foo")
/// - "foo.bar" with level 1 -> (from .foo import bar, "bar")
fn determine_import_strategy(
    module_path: Option<&str>,
    level: u32,
    import_name: &str,
    alias: Option<&str>,
) -> (NameImport, String) {
    let reference_prefix = alias.unwrap_or(import_name).to_string();

    if level > 0 {
        // Relative import: from .foo import bar or from . import foo
        // module_path might be None for `from . import foo`
        if let Some(module) = module_path {
            // from .foo.bar import baz -> from .foo import bar
            let parts: Vec<&str> = module.split('.').collect();
            if parts.len() > 1 {
                // Nested: from .foo.bar import baz
                let base_module = parts[..parts.len() - 1].join(".");
                let import = NameImport::ImportFrom(MemberNameImport {
                    module: Some(base_module),
                    name: Alias {
                        name: import_name.to_string(),
                        as_name: alias.map(String::from),
                    },
                    level,
                });
                (import, reference_prefix)
            } else {
                // Simple: from .foo import bar
                let import = NameImport::ImportFrom(MemberNameImport {
                    module: Some(module.to_string()),
                    name: Alias {
                        name: import_name.to_string(),
                        as_name: alias.map(String::from),
                    },
                    level,
                });
                (import, reference_prefix)
            }
        } else {
            // from . import foo
            let import = NameImport::ImportFrom(MemberNameImport {
                module: None,
                name: Alias {
                    name: import_name.to_string(),
                    as_name: alias.map(String::from),
                },
                level,
            });
            (import, reference_prefix)
        }
    } else {
        // Absolute import
        if let Some(module) = module_path {
            let parts: Vec<&str> = module.split('.').collect();
            if parts.len() > 1 {
                // Nested: from requests.adapters import HTTPAdapter
                // -> from requests import adapters
                let base_module = parts[..parts.len() - 1].join(".");
                let import = NameImport::ImportFrom(MemberNameImport {
                    module: Some(base_module),
                    name: Alias {
                        name: import_name.to_string(),
                        as_name: alias.map(String::from),
                    },
                    level: 0,
                });
                (import, reference_prefix)
            } else {
                // Simple: from requests import get -> import requests
                let import = if let Some(alias_name) = alias {
                    NameImport::Import(ModuleNameImport::alias(
                        import_name.to_string(),
                        alias_name.to_string(),
                    ))
                } else {
                    NameImport::Import(ModuleNameImport::module(import_name.to_string()))
                };
                (import, reference_prefix)
            }
        } else {
            // Edge case: from . import foo but level=0 (shouldn't happen)
            let import = NameImport::Import(ModuleNameImport::module(import_name.to_string()));
            (import, reference_prefix)
        }
    }
}

/// Generate a fix for a non-module import by converting it to a module import
/// and updating all references
fn generate_fix(
    binding: &Binding,
    module_path: &str,
    imported_name: &str,
    level: u32,
    import_scope: &Scope,
    checker: &Checker,
) -> Result<Fix> {
    // Get the import statement
    let Some(statement) = binding.statement(checker.semantic()) else {
        anyhow::bail!("Binding has no source statement");
    };

    // Get parent statement for import removal
    let parent = binding
        .source
        .and_then(|node_id| checker.semantic().parent_statement_id(node_id))
        .map(|node_id| checker.semantic().statement(node_id));

    // Determine the base name for the import
    // For "os.path", we import "path" (from os import path)
    // For "requests.adapters", we import "adapters" (from requests import adapters)
    // For "requests", we import "requests" (import requests)
    let parts: Vec<&str> = module_path.split('.').collect();
    let base_name = if parts.len() > 1 {
        parts[parts.len() - 1] // Last part for nested modules
    } else {
        module_path // Use full path for simple modules
    };

    // For nested modules, we want "from X import Y" not "import X"
    // So for "os.path.join", module_path is "os.path", base_name is "path"
    // We'll generate "from os import path" and references become "path.join"

    // Find a unique name that doesn't shadow anything
    let (import_name, alias, is_reusing_existing) =
        find_unique_import_name(base_name, module_path, binding, import_scope, checker);

    // The prefix to use in references
    let reference_prefix = alias.as_ref().unwrap_or(&import_name);

    // Step 1: Update all references to use the qualified name
    let mut reference_edits = Vec::new();
    for ref_id in binding.references() {
        let reference = checker.semantic().reference(ref_id);
        // Replace "get" with "requests.get" or "requests_mod.get"
        let new_text = format!("{}.{}", reference_prefix, imported_name);
        reference_edits.push(Edit::range_replacement(new_text, reference.range()));
    }

    // Step 2: Add the new import statement (unless we're reusing an existing one)
    let add_import_edit = if is_reusing_existing {
        // We're reusing an existing import, so we don't need to add a new one
        // But we still need an Edit for the structure - use an empty edit at the binding location
        // Actually, we can skip this entirely and not include it in the edits
        None
    } else {
        // Determine what import to create based on module structure
        let (new_import, _) = determine_import_strategy(
            if module_path.is_empty() {
                None
            } else {
                Some(module_path)
            },
            level, // Preserve the relative import level from the original import
            &import_name,
            alias.as_deref(),
        );

        // Add the import after existing imports
        Some(checker.importer().add_import(&new_import, binding.start()))
    };

    // Step 3: Remove the symbol from the original import statement
    // IMPORTANT: Use imported_name (the member_name from the import, e.g., "path")
    // NOT the bound_name (which might be an alias, e.g., "path_mod").
    // The remove_unused_imports function needs to match the actual import statement text.
    // For example, `from os import path as p` should remove "path", not "p".
    let remove_import_edit = remove_unused_imports(
        std::iter::once(imported_name),
        statement,
        parent,
        checker.locator(),
        checker.stylist(),
        checker.indexer(),
    )?;

    // Combine all edits: references + add import (if not reusing) + remove import
    // The first edit should be the most "significant" one for display purposes
    let mut all_edits = Vec::new();

    // If we have reference edits, use the first reference edit as primary
    let has_references = !reference_edits.is_empty();
    if has_references {
        all_edits.push(reference_edits[0].clone());
        all_edits.extend(reference_edits.into_iter().skip(1));
    }

    // Add the import edit if we're not reusing an existing import
    if let Some(import_edit) = add_import_edit {
        if has_references {
            all_edits.push(import_edit);
        } else {
            // No references - use the import addition as primary
            all_edits.insert(0, import_edit);
        }
    }

    // Always add the removal edit
    all_edits.push(remove_import_edit);

    // Ensure we have at least one edit
    if all_edits.is_empty() {
        anyhow::bail!("No edits generated for fix");
    }

    Ok(Fix::unsafe_edits(
        all_edits[0].clone(),
        all_edits[1..].to_vec(),
    ))
}

/// RUF066
pub(crate) fn non_module_import(checker: &Checker, scope: &Scope) {
    let settings = &checker.settings().ruff;

    // Get source directories from settings, plus the directory of the current file
    let mut src_dirs = checker.settings().src.clone();
    if let Some(parent) = checker.path().parent() {
        src_dirs.push(parent.to_path_buf());
    }

    // Iterate over all bindings in the scope
    for binding_id in scope.binding_ids() {
        let binding = checker.semantic().binding(binding_id);

        // Only check ImportFrom bindings
        let Some(import) = binding.as_any_import() else {
            continue;
        };

        // We only care about `from X import Y` style imports
        let import_from = match import {
            ruff_python_semantic::AnyImport::Import(_) => continue,
            ruff_python_semantic::AnyImport::SubmoduleImport(_) => continue,
            ruff_python_semantic::AnyImport::FromImport(import_from) => import_from,
        };

        // Skip if in TYPE_CHECKING block
        if binding.context.is_typing() {
            continue;
        }

        // Extract the import level from the original AST to check for relative imports
        // (e.g., level=1 for "from . import foo", level=2 for "from .. import foo")
        let level = binding
            .source
            .and_then(|node_id| {
                let stmt = checker.semantic().statement(node_id);
                if let Stmt::ImportFrom(import_from_stmt) = stmt {
                    Some(import_from_stmt.level)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        // Skip relative imports - they're complex to handle correctly and we can't
        // easily determine if a relative import target is a module without more context
        if level > 0 {
            continue;
        }

        // Get the module path from the qualified name
        // qualified_name is like "requests.get", we want "requests"
        // or "requests.adapters.HTTPAdapter", we want "requests.adapters"
        let qual_name = import_from.qualified_name();
        let qual_segments = qual_name.segments();

        // Take all but the last segment (last segment is the imported member)
        let module_path = if qual_segments.len() > 1 {
            qual_segments[..qual_segments.len() - 1].join(".")
        } else {
            String::new()
        };

        // Skip if module is in the default allow-list
        if DEFAULT_ALLOWED_MODULES.contains(&module_path.as_str()) {
            continue;
        }

        // Skip if module is in the user-configured allow-list
        if settings
            .non_module_import_allow_modules
            .iter()
            .any(|m| m == &module_path)
        {
            continue;
        }

        // Check first segment of module path to determine import type
        let module_base = module_path.split('.').next().unwrap_or("");
        let is_stdlib = !module_base.is_empty()
            && is_known_standard_library(checker.target_version().minor, module_base);

        // Skip based on import type and settings
        if is_stdlib && !settings.non_module_import_check_stdlib {
            continue;
        }

        // Determine if this is first-party or third-party based on filesystem
        let is_first_party = is_module_on_filesystem(module_base, &src_dirs);

        // Skip based on first-party/third-party settings
        if is_first_party && !settings.non_module_import_check_first_party {
            continue;
        }
        if !is_first_party && !is_stdlib && !settings.non_module_import_check_third_party {
            continue;
        }

        // Build search paths: src_dirs for first-party, third-party paths for third-party
        let search_paths = if is_first_party || is_stdlib {
            src_dirs.clone()
        } else {
            // For third-party, use configured paths
            let mut paths = settings.non_module_import_third_party_module_paths.clone();
            // Also include src_dirs in case they have vendored dependencies
            paths.extend(src_dirs.clone());
            paths
        };

        // Get the imported member name (e.g., "path" from "from os import path as path_mod")
        // This is what appears in the import statement itself, not any alias.
        let member_name = import_from.member_name();

        // Also get the bound name for the diagnostic message
        // This is what the symbol is bound to in the local scope, which may include an alias.
        // For `from os import path as p`, member_name is "path" and bound_name is "p".
        let bound_name = binding.name(checker.source());

        // Build full import path: "module.name"
        let full_path = if module_path.is_empty() {
            member_name.to_string()
        } else {
            format!("{}.{}", module_path, member_name)
        };

        // Check if it's a module:
        // 1. For stdlib, check if the full path is a known stdlib module or submodule
        // 2. Otherwise, check the filesystem
        let is_module = if is_stdlib {
            // For stdlib, check if the full path is a known module
            // This handles cases like `from collections import abc` where abc is a submodule
            is_known_standard_library(checker.target_version().minor, &full_path)
                || KNOWN_STDLIB_SUBMODULES.contains(&full_path.as_str())
        } else {
            // For first-party and third-party, check the filesystem
            is_module_on_filesystem(&full_path, &search_paths)
        };

        if !is_module {
            // It's not a module, report violation
            let mut diagnostic = checker.report_diagnostic(
                NonModuleImport {
                    module: module_path.to_string(),
                    name: bound_name.to_string(),
                },
                binding.range(),
            );

            // Try to generate a fix
            // Pass the member name (not bound name) so remove_unused_imports works correctly
            if let Ok(fix) =
                generate_fix(binding, &module_path, &member_name, level, scope, checker)
            {
                diagnostic.set_fix(fix);
            }
        }
    }
}

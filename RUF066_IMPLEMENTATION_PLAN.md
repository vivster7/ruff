# RUF066 Implementation Plan: Module-Only Imports

## Overview

**Rule Code:** RUF066
**Rule Name:** `NonModuleImport` (following Ruff's "allow X" naming convention)
**Category:** Ruff-specific (`RUF`)
**Purpose:** Enforce that only modules can be imported, not symbols/attributes from modules

### Goal

Enable lazy imports in Python using `importlib.util.LazyLoader` by ensuring all imports are module-level rather than symbol-level.

**Allowed:**
```python
from foo import bar  # OK if foo/bar.py exists
import foo.bar.baz    # OK
```

**Violations:**
```python
from foo.bar import MyClass      # RUF066 - importing symbol
from foo.bar import my_function  # RUF066 - importing symbol
```

## Motivation and Use Case

Python's `importlib.util.LazyLoader` only works with module imports, not symbol imports. This rule ensures codebases that want to use lazy loading follow the required import pattern.

**Why this matters:**
- Lazy loading can significantly improve application startup time
- Currently no tooling enforces the module-only import pattern
- Manual enforcement is error-prone and time-consuming

## Design Decisions

### Decision 1: On-Demand Filesystem Checking vs Pre-Scanning

**Question:** Should we build a cache of all available modules upfront, or check the filesystem for each import as we encounter it?

**Decision:** **On-demand checking** (check filesystem per-import)

**Rationale:**
1. **Architecture Fit**: Ruff processes files in parallel. A global cache would require thread-safe access and complicate the architecture.
2. **Performance**: Filesystem stat() calls are microseconds-fast, and the OS caches results. Even 1000 imports = ~3000 stat calls = negligible.
3. **Simplicity**: No cache invalidation, no shared state, no pre-computation overhead.
4. **Precedent**: Ruff's existing `match_sources()` function (in `isort/categorize.rs:169`) uses on-demand filesystem checking for import categorization.

**Supporting Evidence:**
- Typical file has ~10-20 imports, requiring ~30-60 filesystem checks
- OS filesystem cache makes repeated checks very fast
- Ruff already does this successfully in import categorization

**Trade-offs:**
- ✅ Pro: Simple, fits Ruff's architecture
- ✅ Pro: No memory overhead
- ✅ Pro: Works well with parallel processing
- ❌ Con: Repeated checks if same module imported across many files (mitigated by OS cache)

**Future Optimization:**
If profiling shows this is a bottleneck, we could add per-file caching (within a single file being linted), but this is unlikely to be needed.

---

### Decision 2: Rule Scope (First-party, Stdlib, Third-party)

**Question:** Which imports should we check?

**Decision:** **Check first-party by default, with configuration for stdlib and third-party**

**Rationale:**
1. **First-party is tractable**: We have filesystem access to determine module structure
2. **Stdlib requires knowledge**: Need to know stdlib module structure (can use `ruff_python_stdlib` crate)
3. **Third-party requires site-packages**: Would need to scan virtual environment

**Default Configuration:**
```toml
[tool.ruff.lint.non-module-import]
check-first-party = true
check-stdlib = false
check-third-party = false
```

**Why this default:**
- First-party code is what users control and want to enforce patterns on
- Stdlib checking can be enabled later (many stdlib imports like `from os.path import join` are idiomatic)
- Third-party checking is complex (requires venv scanning) - defer to future work

**Implementation Phases:**
- Phase 1 (MVP): First-party only
- Phase 2: Stdlib support using `ruff_python_stdlib`
- Phase 3: Third-party support (scan venv/site-packages)

---

### Decision 3: Default Allowed Modules

**Question:** Should certain modules be allowed by default even if configured to check?

**Decision:** **Yes - allow `typing`, `__future__`, `typing_extensions`, and `collections.abc` by default**

**Rationale:**
1. **`__future__` imports**: Must be at module level, always import symbols (e.g., `from __future__ import annotations`)
2. **`typing` imports**: Extremely common pattern (`from typing import List, Dict`), used only for type checking
3. **`typing_extensions`**: Backports of typing features, same rationale as `typing`
4. **`collections.abc`**: Common to import abstract base classes directly

**User Override:**
```toml
[tool.ruff.lint.non-module-import]
allow-modules = ["typing", "mypy_extensions", "custom_module"]  # Override defaults
```

**Trade-offs:**
- ✅ Pro: Reduces false positives for idiomatic Python patterns
- ✅ Pro: `typing` imports are erased at runtime, so lazy loading doesn't apply
- ❌ Con: Less strict, but can be overridden by users who want maximum strictness

---

### Decision 4: TYPE_CHECKING Blocks

**Question:** Should we check imports inside `if TYPE_CHECKING:` blocks?

**Decision:** **Skip them (don't check)**

**Rationale:**
1. **Never executed**: `TYPE_CHECKING` is `False` at runtime, so these imports are never executed
2. **Lazy loading irrelevant**: Since they don't run, lazy loading optimization doesn't apply
3. **Type-only imports**: These are for type checkers only, not runtime

**Example:**
```python
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from foo.bar import MyClass  # Skip this - no RUF066
```

**Implementation:**
- Track when inside a `TYPE_CHECKING` block in the semantic model
- Skip `ImportFrom` statements when in this context

**Alternative Considered:**
Checking TYPE_CHECKING blocks for consistency - rejected because it provides no value for the lazy loading use case.

---

### Decision 5: Package Re-exports

**Question:** How to handle symbols re-exported in `__init__.py`?

Example:
```python
# foo/__init__.py
from .bar import MyClass

# other_file.py
from foo import MyClass  # Is this a module or symbol import?
```

**Decision:** **Strict - flag this as a violation (Option A)**

**Rationale:**
1. **Lazy loading perspective**: `MyClass` is technically a symbol, not a module
2. **Filesystem check**: There's no `foo/MyClass.py` file, so it fails the module test
3. **Consistency**: Clear, consistent rule - only actual modules allowed
4. **Workaround available**: Users can do `import foo.bar` then `foo.bar.MyClass`

**Trade-offs:**
- ✅ Pro: Consistent with lazy loading requirements
- ✅ Pro: Clear, simple rule
- ❌ Con: Flags a common Python pattern (but users can disable rule or use `# noqa`)

**Alternative Considered:**
Allow re-exports (Option B) - rejected because it defeats the purpose of enforcing module-only imports for lazy loading.

---

### Decision 6: Wildcard Imports

**Question:** How to handle `from foo.bar import *`?

**Decision:** **Skip them (don't check)**

**Rationale:**
1. **Already problematic**: Wildcard imports are discouraged by other rules (F403, F405)
2. **Ambiguous**: Can't determine what's being imported without runtime analysis
3. **Edge case**: Rare in well-structured codebases
4. **Complexity**: Checking would require parsing `__all__` or analyzing the module

**Example:**
```python
from foo.bar import *  # Skip - no RUF066
```

**Alternative Considered:**
- Always flag as violation - rejected as overly complex and redundant with existing rules
- Could revisit in future if there's demand

---

### Decision 7: Rule Code Selection

**Question:** Which RUF code to use?

**Decision:** **RUF066**

**Rationale:**
- Last regular RUF code is RUF065 (checked `codes.rs`)
- RUF066 is next in sequence
- RUF100+ are reserved for special cases (noqa-related, pyproject.toml, test rules)

---

## Implementation Architecture

### Module Detection Algorithm

For each `from a.b import c` statement:

```rust
fn is_module_import(
    module_path: &str,      // "a.b"
    imported_name: &str,    // "c"
    level: u32,             // 0 for absolute, >0 for relative
    src_dirs: &[PathBuf],   // From settings.src
    package: Option<PackageRoot>,
    current_module: &str,
) -> bool {
    // 1. Resolve relative imports to absolute
    let resolved_module = if level > 0 {
        resolve_relative_import(module_path, level, current_module, package)
    } else {
        module_path.to_string()
    };

    // 2. Build full import path: "a.b.c"
    let full_path = if resolved_module.is_empty() {
        imported_name.to_string()
    } else {
        format!("{}.{}", resolved_module, imported_name)
    };

    // 3. Check filesystem in each src directory
    for src_dir in src_dirs {
        // Convert "a.b.c" to "a/b/c"
        let relative_path: PathBuf = full_path.split('.').collect();
        let candidate = src_dir.join(&relative_path);

        // Is it a package directory?
        if candidate.is_dir() && candidate.join("__init__.py").exists() {
            return true;
        }

        // Is it a module file?
        if candidate.with_extension("py").is_file() {
            return true;
        }

        // Is it a stub file?
        if candidate.with_extension("pyi").is_file() {
            return true;
        }
    }

    // Not found - must be a symbol import
    false
}
```

**Why this works:**
- Converts dotted import path to filesystem path
- Checks all configured source directories
- Handles both packages (directories with `__init__.py`) and modules (`.py` files)
- Handles stub files (`.pyi`)

**Edge cases handled:**
- Relative imports: Converted to absolute before checking
- Multiple src directories: Checks all of them
- Namespace packages: Regular `is_dir()` check works (no `__init__.py` required)

---

### Integration Points

#### 1. Statement Analysis Hook

**Location:** `crates/ruff_linter/src/checkers/ast/analyze/statement.rs`

**Integration:**
```rust
Stmt::ImportFrom(
    import_from @ ast::StmtImportFrom {
        names,
        module,
        level,
        range: _,
        node_index: _,
    },
) => {
    // ... existing rules ...

    if checker.is_rule_enabled(Rule::NonModuleImport) {
        ruff::rules::non_module_import(
            checker,
            import_from,
            module.as_deref(),
            *level,
            names,
        );
    }
}
```

**Why here:**
- This is where all `ImportFrom` statements are processed
- All other import-related rules hook in here
- Access to all necessary context (checker, semantic model, settings)

#### 2. Rule File

**Location:** `crates/ruff_linter/src/rules/ruff/rules/non_module_import.rs`

**Structure:**
```rust
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use std::path::PathBuf;

/// ## What it does
/// Checks for imports of symbols from modules, rather than importing modules directly.
///
/// ## Why is this bad?
/// When using lazy imports with `importlib.util.LazyLoader`, only module imports
/// are supported. Importing symbols directly prevents lazy loading.
///
/// ## Example
/// ```python
/// # Bad
/// from foo.bar import MyClass
///
/// # Good
/// from foo import bar
/// # or
/// import foo.bar
/// ```
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

pub(crate) fn non_module_import(
    checker: &mut Checker,
    import_from: &ast::StmtImportFrom,
    module: Option<&str>,
    level: u32,
    names: &[ast::Alias],
) {
    // Implementation here
}
```

#### 3. Rule Registration

**Location:** `crates/ruff_linter/src/codes.rs`

```rust
(Ruff, "066") => (RuleGroup::Preview, rules::ruff::rules::NonModuleImport),
```

**Why Preview:**
New rules start in `RuleGroup::Preview` per Ruff's contribution guidelines.

#### 4. Configuration Schema

**Location:** `crates/ruff_workspace/src/options.rs`

```rust
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NonModuleImportOptions {
    /// Check first-party imports
    pub check_first_party: Option<bool>,

    /// Check standard library imports
    pub check_stdlib: Option<bool>,

    /// Check third-party imports
    pub check_third_party: Option<bool>,

    /// Module names to allow importing symbols from
    pub allow_modules: Option<Vec<String>>,
}
```

**Location:** `crates/ruff_linter/src/rules/ruff/settings.rs` (or new file)

```rust
#[derive(Debug, Clone, Default)]
pub struct NonModuleImportSettings {
    pub check_first_party: bool,
    pub check_stdlib: bool,
    pub check_third_party: bool,
    pub allow_modules: FxHashSet<String>,
}

impl Default for NonModuleImportSettings {
    fn default() -> Self {
        Self {
            check_first_party: true,
            check_stdlib: false,
            check_third_party: false,
            allow_modules: FxHashSet::from_iter([
                "typing".to_string(),
                "__future__".to_string(),
                "typing_extensions".to_string(),
                "collections.abc".to_string(),
            ]),
        }
    }
}
```

---

## Implementation Plan

### Phase 1: MVP - First-Party Only

**Scope:**
- Check first-party imports only
- Default allow-list (`typing`, `__future__`, etc.)
- Skip TYPE_CHECKING blocks
- Skip wildcard imports
- Basic configuration support

**Files to Create/Modify:**
1. `crates/ruff_linter/src/rules/ruff/rules/non_module_import.rs` - New rule implementation
2. `crates/ruff_linter/src/rules/ruff/rules/mod.rs` - Export new rule
3. `crates/ruff_linter/src/codes.rs` - Register RUF066
4. `crates/ruff_linter/src/checkers/ast/analyze/statement.rs` - Hook into ImportFrom
5. `crates/ruff_workspace/src/options.rs` - Configuration schema
6. `crates/ruff_linter/src/rules/ruff/settings.rs` - Settings struct
7. `crates/ruff_linter/resources/test/fixtures/ruff/RUF066.py` - Test fixtures
8. Test file in `crates/ruff_linter/src/rules/ruff/rules/` - Snapshot tests

**Test Coverage:**
- First-party module imports (should pass)
- First-party symbol imports (should fail)
- Relative imports (both module and symbol)
- Allowed modules (typing, __future__)
- TYPE_CHECKING blocks (should skip)
- Wildcard imports (should skip)
- Configuration options

**Deliverables:**
- Working rule with tests
- Documentation (auto-generated via `cargo dev generate-all`)
- Snapshot tests

### Phase 2: Standard Library Support

**Scope:**
- Add stdlib checking using `ruff_python_stdlib` crate
- Configuration to enable/disable
- Extend allow-list for common stdlib patterns

**Implementation:**
```rust
use ruff_python_stdlib::sys::is_known_standard_library;

// In rule logic:
let module_base = module_path.split('.').next().unwrap();
if is_known_standard_library(target_version.minor, module_base) {
    // Check if we should validate stdlib imports
    if !settings.non_module_import.check_stdlib {
        return;
    }

    // For stdlib, we can't check filesystem
    // Would need hardcoded knowledge of stdlib structure
    // OR mark as unknown and skip
}
```

**Challenge:**
Stdlib module structure varies by Python version. Need to handle this carefully or skip for now.

### Phase 3: Third-Party Support (Future)

**Scope:**
- Scan site-packages/venv for installed packages
- Build module cache for third-party packages
- Configuration to enable/disable

**Challenges:**
- Finding site-packages location
- Handling multiple Python versions
- Performance of scanning large venvs
- Keeping cache updated

**Decision:** Defer to future work, potentially separate RFC/design doc.

---

## Testing Strategy

### Test File Structure

**Location:** `crates/ruff_linter/resources/test/fixtures/ruff/RUF066.py`

```python
"""Test fixtures for RUF066 (non-module-import)"""

# Assume project structure:
# src/
#   foo/
#     __init__.py
#     bar.py (contains: class MyClass, def my_function())
#     baz/
#       __init__.py
#       qux.py

# ============================================================================
# VIOLATIONS: First-party symbol imports
# ============================================================================

from foo.bar import MyClass  # RUF066
from foo.bar import my_function  # RUF066
from foo.baz.qux import SomeClass  # RUF066

# ============================================================================
# OK: First-party module imports
# ============================================================================

from foo import bar  # OK - bar is a module
from foo.baz import qux  # OK - qux is a module
import foo.bar  # OK - import statements are always modules

# ============================================================================
# OK: Relative imports (modules)
# ============================================================================

from . import bar  # OK
from .. import foo  # OK
from .baz import qux  # OK

# ============================================================================
# VIOLATIONS: Relative imports (symbols)
# ============================================================================

from .bar import MyClass  # RUF066
from ..foo.bar import my_function  # RUF066

# ============================================================================
# OK: Allowed modules (default allow-list)
# ============================================================================

from typing import List, Dict, Optional  # OK - typing in allow-list
from __future__ import annotations  # OK - __future__ in allow-list
from typing_extensions import TypedDict  # OK - typing_extensions in allow-list
from collections.abc import Mapping  # OK - collections.abc in allow-list

# ============================================================================
# OK: TYPE_CHECKING blocks (skipped)
# ============================================================================

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from foo.bar import MyClass  # OK - inside TYPE_CHECKING block

# ============================================================================
# OK: Wildcard imports (skipped)
# ============================================================================

from foo.bar import *  # OK - wildcard imports skipped

# ============================================================================
# OK: Standard library (not checked by default)
# ============================================================================

from os.path import join  # OK - check_stdlib=false
from collections import defaultdict  # OK - check_stdlib=false
```

### Snapshot Tests

**Location:** `crates/ruff_linter/src/rules/ruff/rules/non_module_import_test.rs` (or in mod.rs)

```rust
use crate::test::test_snippet;
use crate::registry::Rule;

#[test]
fn test_non_module_import() {
    let diagnostics = test_snippet(
        Rule::NonModuleImport,
        Path::new("RUF066.py"),
        Path::new("ruff"),
    )
    .unwrap();
    insta::assert_snapshot!(diagnostics);
}
```

**Process:**
1. Run `cargo test` - will fail initially
2. Run `cargo insta review` - review and accept snapshots
3. Commit snapshot files with code

### Configuration Tests

Test different configurations:

```rust
#[test]
fn test_check_stdlib_enabled() {
    // Test with check_stdlib=true
}

#[test]
fn test_custom_allow_modules() {
    // Test with custom allow-modules list
}

#[test]
fn test_check_first_party_disabled() {
    // Test with check_first_party=false
}
```

---

## Open Questions & Future Considerations

### 1. Namespace Packages

**Question:** How to handle namespace packages (PEP 420)?

**Current Approach:** `is_dir()` check will work for namespace packages (no `__init__.py` required).

**Potential Issue:** Should we distinguish between regular packages and namespace packages?

**Decision:** Handle them the same way for now. Ruff already has namespace package configuration that we can respect.

### 2. Editable Installs

**Question:** What about editable installs in development?

**Current Approach:** We check `src` directories, which should include editable install locations if configured correctly.

**Recommendation:** Document that users should configure `src` setting to include all relevant source directories.

### 3. Stub Files Only

**Question:** What if a module only has a `.pyi` file, no `.py` file?

**Current Approach:** We check for both `.py` and `.pyi`, so this works.

**Example:** `foo/bar.pyi` (no `foo/bar.py`) - importing from `foo.bar` will be treated as a module.

### 4. Dynamic Imports

**Question:** What about `importlib.import_module()` or `__import__()`?

**Current Approach:** This rule only checks static `from X import Y` statements.

**Rationale:** Dynamic imports are rare and can't be statically analyzed reliably.

### 5. Circular Import Workarounds

**Question:** How does this interact with TYPE_CHECKING patterns used to avoid circular imports?

**Current Approach:** We skip TYPE_CHECKING blocks, so this should be fine.

**Example:**
```python
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from foo import Bar  # Skip this

def my_function() -> "Bar":  # Forward reference as string
    pass
```

---

## Performance Considerations

### Estimated Performance Impact

**Per-file cost:**
- ~10-20 import statements per typical file
- ~3 filesystem stat() calls per import being checked
- **Total: ~30-60 stat() calls per file**

**Filesystem Operations:**
- `is_dir()` - checks if path is directory (~1-5 μs)
- `is_file()` - checks if path is file (~1-5 μs)
- OS caching makes subsequent calls faster

**Projected Impact:**
- For 1000-file codebase: ~30,000-60,000 stat calls
- With modern SSD and OS caching: ~30-100ms total
- **Negligible compared to parsing/AST analysis**

**Mitigation Strategies (if needed):**
1. Per-file caching: Cache results within a single file
2. Batch stat calls: Use `try_exists()` to avoid exceptions
3. Early termination: Stop at first match

**Monitoring:**
- Add to Ruff's benchmark suite
- Profile with real-world codebases
- Optimize only if profiling shows bottleneck

---

## Documentation Requirements

### Rule Documentation (Auto-Generated)

Will be generated by `cargo dev generate-all` from the violation metadata:

```rust
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
/// - `lint.non-module-import.check-first-party`
/// - `lint.non-module-import.check-stdlib`
/// - `lint.non-module-import.check-third-party`
/// - `lint.non-module-import.allow-modules`
```

### Configuration Documentation

In `pyproject.toml`:
```toml
[tool.ruff.lint.non-module-import]
# Check first-party (local) imports (default: true)
check-first-party = true

# Check standard library imports (default: false)
check-stdlib = false

# Check third-party imports (default: false)
check-third-party = false

# Modules to allow importing symbols from (default: ["typing", "__future__", "typing_extensions", "collections.abc"])
allow-modules = ["typing", "mypy_extensions"]
```

### User Guide Addition

Add to Ruff documentation explaining:
- Use case (lazy imports)
- How to enable (--select RUF066 or --preview)
- Configuration options
- Common patterns and workarounds
- Integration with importlib.util.LazyLoader

---

## Assumptions & Risks

### Assumptions

1. **Filesystem Layout Assumption:**
   - **Assumption:** Python module structure matches filesystem structure
   - **Risk:** Dynamic imports, sys.path manipulation, or complex packaging might break this
   - **Mitigation:** Document limitations, support `src` configuration

2. **Source Directory Configuration:**
   - **Assumption:** Users have configured `src` setting correctly
   - **Risk:** Incorrect configuration leads to false positives/negatives
   - **Mitigation:** Use sensible defaults (project root + src/), document clearly

3. **Performance Assumption:**
   - **Assumption:** Filesystem stat() calls are fast enough
   - **Risk:** Network filesystems or very large codebases might be slow
   - **Mitigation:** Monitor performance, add caching if needed

4. **TYPE_CHECKING Detection:**
   - **Assumption:** Can reliably detect `if TYPE_CHECKING:` blocks in semantic model
   - **Risk:** Complex boolean logic might bypass detection
   - **Mitigation:** Conservative approach - only skip simple `if TYPE_CHECKING:` pattern

### Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| False positives due to dynamic imports | Medium | Medium | Document limitations, allow configuration exceptions |
| Performance issues on large codebases | Low | High | Profile early, add caching if needed |
| Stdlib checking too complex | Medium | Low | Defer to Phase 2, make it opt-in |
| Users misconfigure `src` setting | Medium | Medium | Clear documentation, sensible defaults |
| Integration with existing tooling | Low | Low | Follow Ruff patterns, extensive testing |

---

## Success Criteria

### MVP (Phase 1)

- [ ] Rule correctly identifies symbol imports from first-party modules
- [ ] Rule allows module imports from first-party modules
- [ ] Default allow-list works (typing, __future__, etc.)
- [ ] TYPE_CHECKING blocks are skipped
- [ ] Wildcard imports are skipped
- [ ] Configuration options work as expected
- [ ] All tests pass with snapshots
- [ ] Documentation is generated correctly
- [ ] Performance is acceptable (<100ms overhead on 1000-file codebase)

### Phase 2

- [ ] Stdlib checking works when enabled
- [ ] Python version compatibility is handled

### Phase 3 (Future)

- [ ] Third-party checking works when enabled
- [ ] Virtual environment scanning is reliable

---

## References

### Ruff Codebase

- **Import categorization:** `crates/ruff_linter/src/rules/isort/categorize.rs:169` (match_sources function)
- **Import statement handling:** `crates/ruff_linter/src/checkers/ast/analyze/statement.rs:699`
- **Filesystem checking example:** `crates/ruff_linter/src/rules/flake8_builtins/rules/stdlib_module_shadowing.rs`
- **Rule registration:** `crates/ruff_linter/src/codes.rs`
- **Contributing guide:** `CONTRIBUTING.md`

### Python Standards

- **PEP 420:** Namespace Packages
- **PEP 562:** Module `__getattr__` and `__dir__`
- **PEP 690:** Lazy Imports

### Related Tools

- **importlib.util.LazyLoader:** Python's lazy import mechanism
- **isort:** Import sorting (provides import categorization patterns)
- **mypy:** Type checker (TYPE_CHECKING pattern)

---

## Timeline Estimate

### Phase 1: MVP (2-3 days)
- Day 1: Core implementation + basic tests
- Day 2: Configuration + comprehensive tests
- Day 3: Documentation, polish, review

### Phase 2: Stdlib (1-2 days)
- Day 1: Implementation
- Day 2: Testing + edge cases

### Phase 3: Third-party (TBD)
- Requires separate design discussion
- Estimate: 3-5 days

---

## Appendix: Example Usage

### Enabling the Rule

```toml
# pyproject.toml or ruff.toml
[tool.ruff]
select = ["RUF066"]  # Or use --preview flag

[tool.ruff.lint.non-module-import]
check-first-party = true
allow-modules = ["typing", "custom_module"]
```

### Before (Violations)

```python
from myapp.models import User
from myapp.services import EmailService
from myapp.utils import format_date

def send_welcome_email(user: User):
    EmailService.send(user.email, format_date(user.created_at))
```

### After (Fixed)

```python
from myapp import models
from myapp import services
from myapp import utils

def send_welcome_email(user: models.User):
    services.EmailService.send(user.email, utils.format_date(user.created_at))
```

### With Lazy Loading

```python
import importlib.util
import sys

# Lazy load modules
for module_name in ["myapp.models", "myapp.services", "myapp.utils"]:
    spec = importlib.util.find_spec(module_name)
    loader = importlib.util.LazyLoader(spec.loader)
    spec.loader = loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    loader.exec_module(module)

# Now imports are lazy - modules load only when accessed
from myapp import models  # Not loaded yet
from myapp import services  # Not loaded yet

# First access triggers loading
user = models.User()  # models module loads now
```

---

**Document Version:** 1.0
**Last Updated:** 2025-10-05
**Status:** Planning Phase
**Next Step:** Begin Phase 1 implementation

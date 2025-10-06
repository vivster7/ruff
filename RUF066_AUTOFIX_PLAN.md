# RUF066 Autofix Implementation Plan

## Overview

Convert `from X import Y` (where Y is a symbol) to `from X import Z` or `import X` (where Z is a module), then update all references to use the qualified name.

**Goal**: Enable lazy loading by ensuring all imports are module-level, not symbol-level.

## Example Transformations

### Basic Case
```python
# Before
from requests import get
result = get(url)

# After
import requests
result = requests.get(url)
```

### Nested Module Case
```python
# Before
from requests.adapters import HTTPAdapter
adapter = HTTPAdapter()

# After
from requests import adapters
adapter = adapters.HTTPAdapter()
```

### Relative Import Case
```python
# Before
from .foo import bar
x = bar.baz()

# After
from . import foo
x = foo.bar.baz()
```

## Algorithm Overview

For each violation in a `from X import Y` statement:

1. **Determine the import structure** (what to import)
2. **Find a unique name** (handle shadowing)
3. **Generate edits**:
   - Add new import (with alias if needed)
   - Update all references to use qualified name
   - Remove violation from original import (or delete if empty)

## Phase 1: Unique Name Generation

### Strategy

Try these approaches in order until we find an available name:

1. **Base name** - Try the natural import name (`requests`, `adapters`, `foo`)
2. **With `_mod` suffix** - Try `{base}_mod` (`requests_mod`, `adapters_mod`)
3. **With parent context** - Try `{parent}_{base}_mod` (`requests_adapters_mod`)
4. **With incrementing number** - Try `{base}_mod{N}` (`requests_mod2`, `requests_mod3`, ...)

### Availability Check

A name is "available" if:
- ✅ Not a Python keyword
- ✅ Not a builtin (for the target Python version)
- ✅ Available in the import's scope (usually module level)
- ✅ Available in **every scope where references exist**

The last point is critical for handling cases like:
```python
from requests import get

def foo():
    requests = {}  # Local variable
    get(url)       # Would break if we use 'requests' as prefix
```

### Implementation Functions

```rust
/// Find a unique name for the import that doesn't shadow anything
fn find_unique_import_name(
    base_name: &str,           // "requests" or "adapters"
    module_path: &str,         // Full path like "requests.adapters"
    binding: &Binding,         // The binding being fixed
    checker: &Checker,
) -> (String, Option<String>)  // (import_name, optional_alias)

/// Check if name is available across all necessary scopes
fn is_name_available(
    name: &str,
    binding: &Binding,
    semantic: &SemanticModel,
    checker: &Checker,
) -> bool

/// Extract parent context for naming strategy #3
/// "requests.adapters" -> Some("requests_adapters_mod")
fn extract_parent_context(module_path: &str) -> Option<String>
```

## Phase 2: Import Structure Determination

### Cases to Handle

#### Case 1: Simple absolute import
```
from requests import get
→ import requests (or `import requests as requests_mod`)
```

#### Case 2: Nested module import
```
from requests.adapters import HTTPAdapter
→ from requests import adapters (or `from requests import adapters as adapters_mod`)
```

#### Case 3: Relative import (single level)
```
from .foo import bar
→ from . import foo (or `from . import foo as foo_mod`)
```

#### Case 4: Relative import (nested)
```
from ..foo.bar import baz
→ from ..foo import bar (or `from ..foo import bar as bar_mod`)
```

### Import Structure Function

```rust
/// Determine what import to create and what prefix to use
struct ImportStrategy {
    /// The import to add (e.g., NameImport::Import or ImportFrom)
    import: NameImport,
    /// The prefix to use in references (might be an alias)
    reference_prefix: String,
}

fn determine_import_strategy(
    module_path: Option<&str>,
    level: u32,  // For relative imports
    unique_name: &str,
    alias: Option<&str>,
) -> ImportStrategy
```

## Phase 3: Edit Generation

### Edits Required

1. **Add new import** - Use `Importer::add_import()`
2. **Update references** - For each reference, replace with qualified name
3. **Modify original import** - Remove the violation, keep valid module imports
4. **Delete original import** - If no valid imports remain

### Reference Update Logic

```rust
// For each reference to the imported symbol
for ref_id in binding.references() {
    let reference = checker.semantic().reference(ref_id);

    // Generate new reference text
    // e.g., "get" -> "requests.get" or "requests_mod.get"
    let new_text = format!("{}.{}", reference_prefix, symbol_name);

    edits.push(Edit::range_replacement(new_text, reference.range()));
}
```

### Import Modification

This is the most complex part. We need to:

1. Parse the existing `from X import A, B, C` statement
2. Remove violations (e.g., remove `A` if it's a symbol)
3. Keep valid module imports (e.g., keep `B` if it's a module)
4. Regenerate the import statement

Options:
- Use Ruff's `fix::codemods::retain_imports()` (similar to unused_import)
- Generate a deletion if all imports are violations
- Generate a replacement if some imports remain

## Phase 4: Edge Cases

### Multiple Violations in One Statement

```python
from requests import get, post, api  # get, post = violations, api = OK
```

Strategy:
- Process all violations together
- Only add `import requests` once
- Keep `api` in the `from` import
- Update references for both `get` and `post`

### Existing Import Already Present

```python
import requests
from requests import get  # Violation
```

Strategy:
- Check if `import requests` exists using semantic model
- Don't add duplicate import
- Still update references and remove the `from` import

### Aliased Imports

```python
from requests import get as req_get
req_get(url)
```

Strategy:
- The alias is lost (makes this an **unsafe** fix)
- Update references to use qualified name: `requests.get(url)`

### Mixed Import and Reference Styles

```python
from requests import get

# Direct call
get(url)

# Passed to function
map(get, urls)

# Accessed in attribute
obj.getter = get
```

All references need updating, regardless of how they're used.

## Implementation Plan

### Step 1: Helper Functions
- [ ] `is_name_available()` - Check shadowing across all scopes
- [ ] `find_unique_import_name()` - Generate unique name with fallback strategies
- [ ] `extract_parent_context()` - Helper for naming strategy

### Step 2: Import Strategy
- [ ] `determine_import_strategy()` - Figure out what import to create
- [ ] Handle absolute imports
- [ ] Handle relative imports
- [ ] Handle nested modules

### Step 3: Edit Generation
- [ ] Add new import using `Importer`
- [ ] Update all references
- [ ] Remove/modify original import statement
- [ ] Handle multiple violations in one statement

### Step 4: Integration
- [ ] Modify `non_module_import()` to set fix on diagnostic
- [ ] Mark fix as unsafe (due to lost aliases)
- [ ] Add `FixAvailability::Sometimes` to violation metadata

### Step 5: Testing
- [ ] Test basic case (simple import)
- [ ] Test shadowing scenarios (verify unique names)
- [ ] Test nested modules
- [ ] Test relative imports
- [ ] Test multiple violations
- [ ] Test existing import scenarios
- [ ] Test aliased imports

## Fix Safety

The fix should be marked as **UNSAFE** because:
1. **Aliases are lost** - `from X import Y as Z` becomes `X.Y`, not `Z`
2. **Significant structural change** - Changes import style
3. **Potential for subtle bugs** - If shadowing detection has edge cases

## Dependencies

### Ruff Components Used
- `SemanticModel::is_available_in_scope()` - Check shadowing
- `Binding::references()` - Get all references
- `Importer::add_import()` - Add new imports
- `Edit` - Text replacements
- `Fix::unsafe_edits()` - Create unsafe fix

### Similar Patterns to Follow
- `renamer.rs` - Reference updating pattern
- `unused_import.rs` - Import manipulation
- `ShadowedKind` - Shadowing detection
- `private_type_parameter.rs` - Fallback when shadowing detected

## Success Criteria

✅ All symbol imports can be converted to module imports
✅ Shadowing conflicts are detected and handled via aliasing
✅ All references are correctly updated
✅ Original imports are properly modified/removed
✅ Works with nested modules and relative imports
✅ Handles multiple violations in single statement
✅ Fix is marked as unsafe but always available

"""Test fixtures for RUF066 (non-module-import)"""

# Assume project structure (created in /tmp/ruff_test_project):
# foo/
#   __init__.py
#   bar.py (contains: class MyClass, def my_function())
#   baz/
#     __init__.py
#     qux.py

# ============================================================================
# VIOLATIONS: First-party symbol imports
# ============================================================================

from foo.bar import MyClass  # RUF066
# Fix: Removes this line only (same as my_function below)

from foo.bar import my_function  # RUF066
# Fix: Removes this line only (does NOT add `import foo.bar` because the rule
# can't detect that foo.bar exists in the filesystem during the test)

from foo.baz.qux import SomeClass  # RUF066
# Fix: Removes this line only (same reason as above)

# ============================================================================
# OK: First-party module imports
# ============================================================================

from foo import bar  # OK - bar is a module
from foo.baz import qux  # OK - qux is a module
import foo.bar  # OK - import statements are always modules

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
    from foo.bar import AnotherClass  # OK - inside TYPE_CHECKING block

# ============================================================================
# OK: Wildcard imports (skipped)
# ============================================================================

from foo.bar import *  # OK - wildcard imports skipped

# ============================================================================
# OK: Standard library (not checked by default)
# ============================================================================

from os.path import join  # OK - check_stdlib=false
from collections import defaultdict  # OK - check_stdlib=false

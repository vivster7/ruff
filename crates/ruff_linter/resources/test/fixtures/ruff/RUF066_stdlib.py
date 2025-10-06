"""Test fixtures for RUF066 stdlib checking"""

# Standard library symbol imports (violations when check_stdlib=true)
from os.path import join  # RUF066 (with check_stdlib)
# Fix: Removes this line and adds `import os.path` (not visible in minimal snapshot)

from collections import defaultdict  # RUF066 (with check_stdlib)
# Fix: Removes this line and adds `import collections`

from json import loads  # RUF066 (with check_stdlib)
# Fix: Removes this line and adds `import json`

# Python 3.9+ modules (violations when check_stdlib=true and py39+)
from graphlib import TopologicalSorter  # RUF066 (with check_stdlib, py39+)
from zoneinfo import ZoneInfo  # RUF066 (with check_stdlib, py39+)

# Python 3.11+ modules (violations when check_stdlib=true and py311+)
from tomllib import load  # RUF066 (with check_stdlib, py311+)

# These are OK because os.path and collections.abc are recognized as stdlib submodules
from os import path  # OK - os.path is a known stdlib submodule
from collections import abc  # OK - collections.abc is a known stdlib submodule

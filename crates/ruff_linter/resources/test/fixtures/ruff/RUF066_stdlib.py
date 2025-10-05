"""Test fixtures for RUF066 stdlib checking"""

# Standard library symbol imports (violations when check_stdlib=true)
from os.path import join  # RUF066 (with check_stdlib)
from collections import defaultdict  # RUF066 (with check_stdlib)
from json import loads  # RUF066 (with check_stdlib)

# Python 3.9+ modules (violations when check_stdlib=true and py39+)
from graphlib import TopologicalSorter  # RUF066 (with check_stdlib, py39+)
from zoneinfo import ZoneInfo  # RUF066 (with check_stdlib, py39+)

# Python 3.11+ modules (violations when check_stdlib=true and py311+)
from tomllib import load  # RUF066 (with check_stdlib, py311+)

# These are also violations because os.path and collections.abc are submodules,
# not in our filesystem, so we can't detect them as modules
from os import path  # RUF066 (with check_stdlib) - stdlib submodule
from collections import abc  # RUF066 (with check_stdlib) - not in default allow-list as import target

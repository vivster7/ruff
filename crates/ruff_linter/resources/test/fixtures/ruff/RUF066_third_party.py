"""Test fixtures for RUF066 third-party checking"""

# ============================================================================
# VIOLATIONS: Third-party symbol imports (when check-third-party=true)
# ============================================================================

from requests import get  # RUF066 - get is a function in __init__.py
from requests import post  # RUF066 - post is a function in __init__.py
from flask import Flask  # RUF066 - Flask is a class in __init__.py
from flask import render_template  # RUF066 - render_template is a function

# ============================================================================
# OK: Third-party module imports
# ============================================================================

from requests import api  # OK - api.py exists (module)
from requests import models  # OK - models.py exists (module)
from flask import views  # OK - views.py exists (module)

# ============================================================================
# OK: Import statements always import modules
# ============================================================================

import requests  # OK
import requests.api  # OK
import flask.views  # OK

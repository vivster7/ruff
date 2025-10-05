//! Settings for the `ruff` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub parenthesize_tuple_in_subscript: bool,
    pub non_module_import_check_first_party: bool,
    pub non_module_import_check_stdlib: bool,
    pub non_module_import_check_third_party: bool,
    pub non_module_import_allow_modules: Vec<String>,
    pub non_module_import_third_party_module_paths: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            parenthesize_tuple_in_subscript: false,
            non_module_import_check_first_party: true,
            non_module_import_check_stdlib: false,
            non_module_import_check_third_party: false,
            non_module_import_allow_modules: Vec::new(),
            non_module_import_third_party_module_paths: Vec::new(),
        }
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.ruff",
            fields = [
                self.parenthesize_tuple_in_subscript,
                self.non_module_import_check_first_party,
                self.non_module_import_check_stdlib,
                self.non_module_import_check_third_party,
                self.non_module_import_allow_modules | array,
                self.non_module_import_third_party_module_paths | debug,
            ]
        }
        Ok(())
    }
}

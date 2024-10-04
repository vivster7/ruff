use ruff_python_ast::PySourceType;
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_parser::typing::ParsedAnnotation;
use ruff_python_semantic::{Module, ModuleKind, ModuleSource};
use ruff_source_file::Locator;
use std::path::Path;

use crate::{
    checkers::ast::Checker,
    noqa::NoqaMapping,
    settings::{flags::Noqa, LinterSettings},
    source_kind::SourceKind,
};

pub fn process_path(path: &Path) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "py") {
                process_file(&path);
            }
        }
    } else if path.is_file() {
        process_file(path);
    }
}

fn process_file(path: &Path) {
    let source_type = PySourceType::from(path);
    let Ok(Some(source_kind)) = SourceKind::from_path(path, source_type) else {
        panic!(
            "Error: Could not determine source type for file: {}",
            path.display()
        )
    };
    let parsed = ruff_python_parser::parse_unchecked_source(source_kind.source_code(), source_type);
    let locator = Locator::new(source_kind.source_code());
    let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
    let indexer = Indexer::from_tokens(parsed.tokens(), &locator);

    let linter_settings = LinterSettings::default();
    let noqa_mapping = NoqaMapping::default();
    let allocator: typed_arena::Arena<ParsedAnnotation> = typed_arena::Arena::new();
    let mut checker = Checker::new(
        &parsed,
        &allocator,
        &linter_settings,
        &noqa_mapping,
        Noqa::Enabled,
        path,
        None,
        Module {
            kind: ModuleKind::Module,
            name: None,
            source: ModuleSource::File(path),
            python_ast: parsed.suite(),
        },
        &locator,
        &stylist,
        &indexer,
        source_type,
        None,
        None,
    );

    checker.visit_all(parsed.suite());

    // for each binding in the global scope print the name of the binding
    let global_scope = checker.semantic().global_scope();
    for (name, binding_id) in global_scope.bindings() {
        let binding = checker.semantic().binding(binding_id);
        println!("{:?} {:?}", binding, binding.name(&locator));
    }

    // for b in checker.semantic().bindings.iter() {
    //     println!("{:?}", b);
    //     println!("{:?}", b.source);
    //     println!("{:?}", b.references());
    // }

    // for d in checker.semantic().definitions.iter() {
    //     println!("{:?}", d.name());
    // }

    // // print references
    // for reference in checker.semantic().resolved_references.iter() {
    //     println!("{:?}", reference);
    //     let scope = checker.semantic().scopes[reference.scope_id()];
    //     let binding = scope.get(reference.name());
    //     let definition_node = binding.definition_node();
    // }

    // // Check docstrings, bindings, and unresolved references.
    // analyze::deferred_lambdas(&mut checker);
    // analyze::deferred_for_loops(&mut checker);
    // analyze::definitions(&mut checker);
    // analyze::bindings(&mut checker);
    // analyze::unresolved_references(&mut checker);

    // // Reset the scope to module-level, and check all consumed scopes.
    // checker.semantic.scope_id = ScopeId::global();
    // checker.analyze.scopes.push(ScopeId::global());
    // analyze::deferred_scopes(&mut checker);
}

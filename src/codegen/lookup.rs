//! Function lookup utilities for JSON → Typst conversion.

use typst::Library;
use typst::foundations::{Func, Module};

/// Find all element functions with the given name.
///
/// Searches in:
/// - Global scope (e.g., `heading`, `strong`)
/// - Module scope (e.g., `math.root`, `math.frac`)
/// - Element sub-scope (e.g., `grid.cell`, `table.cell`, `outline.entry`)
///
/// Non-element functions (e.g., `calc.root`) are filtered out.
pub fn find_element_funcs<'a>(
    library: &'a Library,
    func_name: &'a str,
) -> impl Iterator<Item = Func> + 'a {
    find_all_funcs(library, func_name).filter(|func| func.element().is_some())
}

/// Find an element function in a parent element's scope.
///
/// This is used for context-aware lookup of sub-elements like `grid.cell` or `table.cell`.
/// When deserializing children of a parent element, we should look for child elements
/// in the parent's scope first.
pub fn find_element_in_scope(parent_func: &Func, func_name: &str) -> Option<Func> {
    parent_func
        .scope()?
        .get(func_name)?
        .read()
        .clone()
        .cast::<Func>()
        .ok()
        .filter(|f| f.element().is_some())
}

/// Find all functions with the given name across all scopes.
pub fn find_all_funcs<'a>(
    library: &'a Library,
    func_name: &'a str,
) -> impl Iterator<Item = Func> + 'a {
    let global_scope = library.global.scope();

    // Global scope functions
    let global_func = global_scope
        .get(func_name)
        .and_then(|binding| binding.read().clone().cast::<Func>().ok());

    // Module scope functions (e.g., math.root)
    let module_funcs = global_scope.iter().filter_map(move |(_, binding)| {
        let module = binding.read().clone().cast::<Module>().ok()?;
        let func_binding = module.scope().get(func_name)?;
        func_binding.read().clone().cast::<Func>().ok()
    });

    // Element sub-scope functions (e.g., grid.cell, table.cell)
    let sub_scope_funcs = global_scope.iter().flat_map(move |(_, binding)| {
        let func = binding.read().clone().cast::<Func>().ok();
        func.into_iter().flat_map(move |f| {
            f.scope().into_iter().flat_map(move |scope| {
                scope
                    .get(func_name)
                    .and_then(|b| b.read().clone().cast::<Func>().ok())
            })
        })
    });

    global_func
        .into_iter()
        .chain(module_funcs)
        .chain(sub_scope_funcs)
}

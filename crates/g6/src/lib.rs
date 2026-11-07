//! Rust port skeleton mirroring SVGO's XAST pipeline:
//! `parse_svg` -> plugin(s) -> `stringify_svg`.

pub mod parser;
pub mod plugins;
pub mod stringifier;
pub mod tools;
pub mod xast;

use xast::XastRoot;

/// A plugin in the JS sense (`lib/types.ts` `Plugin`): a named transform
/// of the XAST root.
pub trait Plugin {
    fn name(&self) -> &'static str;
    fn apply(&self, root: &mut XastRoot);
}

pub struct RemoveUnusedNs;
impl Plugin for RemoveUnusedNs {
    fn name(&self) -> &'static str { plugins::remove_unused_ns::NAME }
    fn apply(&self, root: &mut XastRoot) { plugins::remove_unused_ns::apply(root) }
}

pub struct RemoveTitle;
impl Plugin for RemoveTitle {
    fn name(&self) -> &'static str { plugins::remove_title::NAME }
    fn apply(&self, root: &mut XastRoot) { plugins::remove_title::apply(root) }
}

pub struct RemoveDimensions;
impl Plugin for RemoveDimensions {
    fn name(&self) -> &'static str { plugins::remove_dimensions::NAME }
    fn apply(&self, root: &mut XastRoot) { plugins::remove_dimensions::apply(root) }
}

#[derive(Default)]
pub struct RemoveDesc(pub plugins::remove_desc::RemoveDescParams);
impl Plugin for RemoveDesc {
    fn name(&self) -> &'static str { plugins::remove_desc::NAME }
    fn apply(&self, root: &mut XastRoot) { plugins::remove_desc::apply(root, &self.0) }
}

#[derive(Default)]
pub struct CleanupListOfValues(pub plugins::cleanup_list_of_values::CleanupListOfValuesParams);
impl Plugin for CleanupListOfValues {
    fn name(&self) -> &'static str { plugins::cleanup_list_of_values::NAME }
    fn apply(&self, root: &mut XastRoot) { plugins::cleanup_list_of_values::apply(root, &self.0) }
}

/// Parse, run one plugin, and stringify (pretty), mirroring the JS
/// fixture harness in `test/plugins/_index.test.js`.
pub fn optimize_with_plugin(input: &str, plugin: &dyn Plugin) -> Result<String, String> {
    let mut root = parser::parse_svg(input)?;
    plugin.apply(&mut root);
    Ok(stringifier::stringify_svg(
        &root,
        &stringifier::StringifyOptions { pretty: true, ..Default::default() },
    ))
}

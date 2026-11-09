pub mod convert_ellipse_to_circle;
pub mod convert_shape_to_path;
pub mod move_elems_attrs_to_group;
pub mod move_group_attrs_to_elems;
pub mod remove_empty_text;
pub mod remove_hidden_elems;

use crate::xast::Root;

pub trait Plugin {
    fn name(&self) -> &'static str;
    fn apply(&self, root: &mut Root);
}
pub fn js_number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() { return 0.0; }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

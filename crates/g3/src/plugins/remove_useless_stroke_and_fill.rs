//! Port of plugins/removeUselessStrokeAndFill.js
//!
//! The JS plugin deoptimises when any `<style>` element or script-bearing
//! node exists, so the stylesheet collected by lib/style.js is always empty
//! in the path that runs. Computed styles therefore reduce to presentation
//! attributes + inline `style` declarations on the node and its ancestors
//! (with inheritance), which is what is implemented here.

use crate::collections::{
    INHERITABLE_ATTRS, PRESENTATION_ATTRS, PRESENTATION_NONINHERITABLE_GROUP_ATTRS, SHAPE_ELEMS,
};
use crate::tools::has_scripts;
use crate::xast::{any_element, Child, Element, Root};
use crate::Params;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RemoveUselessStrokeAndFillParams {
    pub stroke: bool,
    pub fill: bool,
    pub remove_none: bool,
}
impl Default for RemoveUselessStrokeAndFillParams {
    fn default() -> Self {
        Self { stroke: true, fill: true, remove_none: false }
    }
}
impl From<&Params> for RemoveUselessStrokeAndFillParams {
    fn from(p: &Params) -> Self {
        let mut d = Self::default();
        if let Some(v) = p.bool("stroke") { d.stroke = v; }
        if let Some(v) = p.bool("fill") { d.fill = v; }
        if let Some(v) = p.bool("removeNone") { d.remove_none = v; }
        d
    }
}

/// lib/style.js computed style values are all `static` here (no stylesheet,
/// hence no `dynamic` rules), so a plain string map is sufficient.
type Style = HashMap<String, String>;

fn parse_inline_style(style: &str) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    for chunk in style.split(';') {
        let chunk = chunk.trim();
        if chunk.is_empty() { continue; }
        if let Some((prop, val)) = chunk.split_once(':') {
            let val = val.trim();
            let (val, important) = match val.strip_suffix("!important") {
                Some(v) => (v.trim(), true),
                None => (val, false),
            };
            out.push((prop.trim().to_string(), val.to_string(), important));
        }
    }
    out
}

/// computeOwnStyle with an empty stylesheet: presentation attributes, then
/// inline declarations overriding per the important-style rules in style.js.
fn compute_own(node: &Element) -> Style {
    let mut computed: Style = HashMap::new();
    let mut important: HashMap<String, bool> = HashMap::new();
    for (name, value) in &node.attributes {
        if PRESENTATION_ATTRS.contains(&name.as_str()) {
            computed.insert(name.clone(), value.clone());
            important.insert(name.clone(), false);
        }
    }
    if let Some(style) = node.get("style") {
        for (name, value, imp) in parse_inline_style(style) {
            let dominated = computed.contains_key(&name)
                && !(imp || important.get(&name) == Some(&false));
            if !dominated {
                computed.insert(name.clone(), value);
                important.insert(name, imp);
            }
        }
    }
    computed
}

/// computeStyle: own style plus inheritable styles from ancestors,
/// nearest ancestor first.
fn compute_style(node: &Element, ancestors: &[Element]) -> Style {
    let mut computed = compute_own(node);
    for anc in ancestors.iter().rev() {
        let own = compute_own(anc);
        for (name, value) in own {
            if !computed.contains_key(&name)
                && INHERITABLE_ATTRS.contains(&name.as_str())
                && !PRESENTATION_NONINHERITABLE_GROUP_ATTRS.contains(&name.as_str())
            {
                computed.insert(name, value);
            }
        }
    }
    computed
}

pub fn apply(root: &mut Root, params: &RemoveUselessStrokeAndFillParams) {
    let deopt = any_element(&root.children, &|el| el.name == "style" || has_scripts(el));
    if deopt {
        return;
    }
    let mut ancestors: Vec<Element> = Vec::new();
    visit_children(&mut root.children, &mut ancestors, params);
}

fn visit_children(children: &mut Vec<Child>, ancestors: &mut Vec<Element>, params: &RemoveUselessStrokeAndFillParams) {
    let mut i = 0;
    while i < children.len() {
        let mut detach = false;
        if let Child::Element(node) = &mut children[i] {
            if node.has("id") {
                // visitSkip: whole subtree deoptimised
            } else {
                if SHAPE_ELEMS.contains(&node.name.as_str()) {
                    detach = process_shape(node, ancestors, params);
                }
                if !detach {
                    ancestors.push(node.clone());
                    visit_children(&mut node.children, ancestors, params);
                    ancestors.pop();
                }
            }
        }
        if detach {
            children.remove(i);
        } else {
            i += 1;
        }
    }
}

/// Returns true when removeNone detaches the node from its parent.
fn process_shape(node: &mut Element, ancestors: &[Element], params: &RemoveUselessStrokeAndFillParams) -> bool {
    let computed = compute_style(node, ancestors);
    let parent_computed = ancestors.last().map(|p| compute_style(p, &ancestors[..ancestors.len() - 1]));
    let get = |m: &Style, k: &str| m.get(k).cloned();
    let stroke = get(&computed, "stroke");
    let stroke_opacity = get(&computed, "stroke-opacity");
    let stroke_width = get(&computed, "stroke-width");
    let marker_end = get(&computed, "marker-end");
    let fill = get(&computed, "fill");
    let fill_opacity = get(&computed, "fill-opacity");
    let parent_stroke = parent_computed.as_ref().and_then(|m| get(m, "stroke"));

    if params.stroke {
        let stroke_none = stroke.is_none() || stroke.as_deref() == Some("none");
        let opacity_zero = stroke_opacity.as_deref() == Some("0");
        let width_zero = stroke_width.as_deref() == Some("0");
        if (stroke_none || opacity_zero || width_zero) && (width_zero || marker_end.is_none()) {
            for name in node.attr_names() {
                if name.starts_with("stroke") {
                    node.remove(&name);
                }
            }
            if let Some(ps) = &parent_stroke {
                if ps != "none" {
                    node.set("stroke", "none");
                }
            }
        }
    }

    if params.fill {
        let fill_none = fill.as_deref() == Some("none");
        let fill_op_zero = fill_opacity.as_deref() == Some("0");
        if fill_none || fill_op_zero {
            for name in node.attr_names() {
                if name.starts_with("fill-") {
                    node.remove(&name);
                }
            }
            if fill.is_none() || fill.as_deref().is_some_and(|f| f != "none") {
                node.set("fill", "none");
            }
        }
    }

    if params.remove_none {
        let stroke_gone = stroke.is_none() || node.get("stroke") == Some("none");
        let fill_gone = fill.as_deref() == Some("none") || node.get("fill") == Some("none");
        if stroke_gone && fill_gone {
            return true;
        }
    }
    false
}

use crate::collections::non_rendering;
use crate::path_data::parse_path_data;
use crate::plugins::Plugin;
use crate::style::{collect_stylesheet, compute_style, has_visible_descendant, Computed, Stylesheet};
use crate::tools::{find_references, has_scripts};
use crate::xast::{detach_by_uid, find_by_uid, Element, Node, Root};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub struct RemoveHiddenElemsParams {
    pub is_hidden: bool, pub display_none: bool, pub opacity0: bool, pub circle_r0: bool,
    pub ellipse_rx0: bool, pub ellipse_ry0: bool, pub rect_width0: bool, pub rect_height0: bool,
    pub pattern_width0: bool, pub pattern_height0: bool, pub image_width0: bool, pub image_height0: bool,
    pub path_empty_d: bool, pub polyline_empty_points: bool, pub polygon_empty_points: bool,
}
impl Default for RemoveHiddenElemsParams {
    fn default() -> Self { Self { is_hidden: true, display_none: true, opacity0: true, circle_r0: true, ellipse_rx0: true, ellipse_ry0: true, rect_width0: true, rect_height0: true, pattern_width0: true, pattern_height0: true, image_width0: true, image_height0: true, path_empty_d: true, polyline_empty_points: true, polygon_empty_points: true } }
}
#[derive(Default)]
pub struct RemoveHiddenElems { pub params: RemoveHiddenElemsParams }

fn is_static(computed: &HashMap<String, Computed>, key: &str, val: &str) -> bool {
    matches!(computed.get(key), Some(Computed::Static(v)) if v==val)
}
struct PhaseA { removals: Vec<u64>, deferred: Vec<u64> }
fn phase_a<'a>(children: &'a [Node], ancestors: &mut Vec<&'a Element>, ss: &Stylesheet, p: &RemoveHiddenElemsParams, out: &mut PhaseA) {
    for c in children {
        if let Node::Element(e)=c {
            if non_rendering(&e.name) { out.deferred.push(e.uid); continue; }
            let computed = compute_style(ss, e, ancestors);
            if p.opacity0 && is_static(&computed, "opacity", "0") {
                if e.name=="path" { out.deferred.push(e.uid); } else { out.removals.push(e.uid); }
                continue;
            }
            ancestors.push(e);
            phase_a(&e.children, ancestors, ss, p, out);
            ancestors.pop();
        }
    }
}
struct MainState {
    removals: Vec<u64>, removed_def_ids: Vec<String>, all_defs: Vec<u64>,
    all_references: HashSet<String>, references_by_id: HashMap<String, Vec<u64>>, deoptimized: bool,
}
#[allow(clippy::too_many_arguments)]
fn main_walk<'a>(children: &'a [Node], parent_name: Option<&str>, ancestors: &mut Vec<&'a Element>, ss: &Stylesheet, p: &RemoveHiddenElemsParams, st: &mut MainState) {
    for c in children {
        if let Node::Element(e)=c {
            // style/hasScripts
            if (e.name=="style" && !e.children.is_empty()) || has_scripts(e) { st.deoptimized = true; }
            if e.name=="defs" { st.all_defs.push(e.uid); }
            if e.name=="use" {
                for (attr, value) in &e.attributes {
                    if attr!="href" && !attr.ends_with(":href") { continue; }
                    let id = if !value.is_empty() { value[1..].to_string() } else { String::new() };
                    st.references_by_id.entry(id).or_default().push(e.uid);
                }
            }
            let mut remove = false;
            if p.circle_r0 && e.name=="circle" && e.children.is_empty() && e.get("r")==Some("0") { remove=true; }
            if !remove && p.ellipse_rx0 && e.name=="ellipse" && e.children.is_empty() && e.get("rx")==Some("0") { remove=true; }
            if !remove && p.ellipse_ry0 && e.name=="ellipse" && e.children.is_empty() && e.get("ry")==Some("0") { remove=true; }
            if !remove && p.rect_width0 && e.name=="rect" && e.children.is_empty() && e.get("width")==Some("0") { remove=true; }
            if !remove && p.rect_height0 && p.rect_width0 && e.name=="rect" && e.children.is_empty() && e.get("height")==Some("0") { remove=true; }
            if !remove && p.pattern_width0 && e.name=="pattern" && e.get("width")==Some("0") { remove=true; }
            if !remove && p.pattern_height0 && e.name=="pattern" && e.get("height")==Some("0") { remove=true; }
            if !remove && p.image_width0 && e.name=="image" && e.get("width")==Some("0") { remove=true; }
            if !remove && p.image_height0 && e.name=="image" && e.get("height")==Some("0") { remove=true; }
            if !remove && p.polyline_empty_points && e.name=="polyline" && !e.has("points") { remove=true; }
            if !remove && p.polygon_empty_points && e.name=="polygon" && !e.has("points") { remove=true; }
            let computed = compute_style(ss, e, ancestors);
            if !remove && p.is_hidden && is_static(&computed, "visibility", "hidden") && !has_visible_descendant(e) { remove=true; }
            if !remove && p.display_none && is_static(&computed, "display", "none") && e.name!="marker" { remove=true; }
            if !remove && p.path_empty_d && e.name=="path" {
                if !e.has("d") { remove=true; }
                else {
                    let pd = parse_path_data(e.get("d").unwrap());
                    if pd.is_empty() || (pd.len()==1 && !computed.contains_key("marker-start") && !computed.contains_key("marker-end")) { remove=true; }
                }
            }
            if remove {
                if let Some(id)=e.get("id") { if parent_name==Some("defs") { st.removed_def_ids.push(id.to_string()); } }
                st.removals.push(e.uid);
                continue;
            }
            for (name, value) in &e.attributes {
                for id in find_references(name, value) { st.all_references.insert(id); }
            }
            ancestors.push(e);
            main_walk(&e.children, Some(&e.name.clone()), ancestors, ss, p, st);
            ancestors.pop();
        }
    }
}
fn can_remove(node: &Element, refs: &HashSet<String>) -> bool {
    if let Some(id)=node.get("id") { if refs.contains(id) { return false; } }
    for c in &node.children { if let Node::Element(ch)=c { if !can_remove(ch, refs) { return false; } } }
    true
}
impl Plugin for RemoveHiddenElems {
    fn name(&self) -> &'static str { "removeHiddenElems" }
    fn apply(&self, root: &mut Root) {
        let p = self.params;
        let ss = collect_stylesheet(root);
        // phase A
        let mut pa = PhaseA { removals: vec![], deferred: vec![] };
        { let mut anc: Vec<&Element> = Vec::new(); phase_a(&root.children, &mut anc, &ss, &p, &mut pa); }
        for uid in &pa.removals { detach_by_uid(&mut root.children, *uid); }
        // main
        let mut st = MainState { removals: vec![], removed_def_ids: vec![], all_defs: vec![], all_references: HashSet::new(), references_by_id: HashMap::new(), deoptimized: false };
        { let mut anc: Vec<&Element> = Vec::new(); main_walk(&root.children, None, &mut anc, &ss, &p, &mut st); }
        for uid in &st.removals { detach_by_uid(&mut root.children, *uid); }
        // root exit
        for id in &st.removed_def_ids {
            if let Some(refs) = st.references_by_id.get(id).cloned() { for uid in refs { detach_by_uid(&mut root.children, uid); } }
        }
        if !st.deoptimized {
            for uid in &pa.deferred {
                let can = if let Some(node)=find_by_uid(&root.children, *uid) { can_remove(node, &st.all_references) } else { false };
                if can { detach_by_uid(&mut root.children, *uid); }
            }
        }
        for uid in &st.all_defs {
            let empty = if let Some(node)=find_by_uid(&root.children, *uid) { node.children.is_empty() } else { false };
            if empty { detach_by_uid(&mut root.children, *uid); }
        }
    }
}

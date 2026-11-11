use crate::dom::{is_text_elem, Document, Element, Node};
use crate::json::Json;
use crate::path::{intersects, js2path, path2js, PathItem};
use crate::style::{collect_stylesheet, compute_prop, has_filter, Stylesheet};
use crate::tools::{find_references, includes_url_reference};
use crate::transforms::{
    js2transform, matrix_to_transform, round_transform, transform2js, transforms_multiply,
    TransformItem, TransformParams,
};

pub mod convert_path_data;

const ANIMATION: &[&str] = &["animate", "animateColor", "animateMotion", "animateTransform", "set"];
const CONTAINER: &[&str] = &[
    "a", "defs", "foreignObject", "g", "marker", "mask", "missing-glyph", "pattern", "svg",
    "switch", "symbol",
];
const CONDITIONAL: &[&str] = &["requiredExtensions", "requiredFeatures", "systemLanguage"];
const INHERITABLE: &[&str] = &[
    "clip-rule", "color-interpolation-filters", "color-interpolation", "color-profile",
    "color-rendering", "color", "cursor", "direction", "dominant-baseline", "fill-opacity",
    "fill-rule", "fill", "font-family", "font-size-adjust", "font-size", "font-stretch",
    "font-style", "font-variant", "font-weight", "font", "glyph-orientation-horizontal",
    "glyph-orientation-vertical", "image-rendering", "letter-spacing", "marker-end",
    "marker-mid", "marker-start", "marker", "paint-order", "pointer-events", "shape-rendering",
    "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin",
    "stroke-miterlimit", "stroke-opacity", "stroke-width", "stroke", "text-anchor",
    "text-rendering", "transform", "visibility", "word-spacing", "writing-mode",
];

pub fn run(doc: &mut Document, name: &str, params: &Json) {
    match name {
        // Task brief spells it "collapsibleGroups"; SVGO's plugin name is "collapseGroups".
        "collapseGroups" | "collapsibleGroups" => collapse_groups(doc),
        "removeEmptyAttrs" => remove_empty_attrs(doc),
        "removeEmptyContainers" => remove_empty_containers(doc),
        "convertTransform" => convert_transform_plugin(doc, params),
        "convertPathData" => convert_path_data::convert_path_data(doc, params),
        "mergePaths" => merge_paths(doc, params),
        _ => {}
    }
}

fn significant(children: &[Node]) -> Vec<Node> {
    children
        .iter()
        .filter(|n| !matches!(n, Node::Text(t) if t.trim().is_empty()))
        .cloned()
        .collect()
}

fn walk_mut<F: FnMut(&mut Element)>(nodes: &mut [Node], f: &mut F) {
    for n in nodes.iter_mut() {
        if let Node::Element(el) = n {
            f(el);
            walk_mut(&mut el.children, f);
        }
    }
}

// ---------- removeEmptyAttrs ----------
fn remove_empty_attrs(doc: &mut Document) {
    walk_mut(&mut doc.children, &mut |el| {
        el.attrs.retain(|(k, v)| !(v.is_empty() && !CONDITIONAL.contains(&k.as_str())));
    });
}

// ---------- removeEmptyContainers ----------
fn collect_uses(nodes: &[Node], uses: &mut Vec<(Vec<String>, String)>) {
    // record (referenced ids) per use element via its key attr snapshot; removal done by matching
    for n in nodes {
        if let Node::Element(el) = n {
            if el.name == "use" {
                let mut ids = Vec::new();
                for (k, v) in &el.attrs {
                    ids.extend(find_references(k, v));
                }
                if !ids.is_empty() {
                    // identify the use element by its serialized attrs + d-less fingerprint
                    uses.push((ids, format!("{:?}", el.attrs)));
                }
            }
            collect_uses(&el.children, uses);
        }
    }
}

fn remove_empty_containers(doc: &mut Document) {
    let sheet = collect_stylesheet(doc);
    let mut uses: Vec<(Vec<String>, String)> = Vec::new();
    collect_uses(&doc.children, &mut uses);
    let mut removed_ids: Vec<String> = Vec::new();
    prune_containers(&mut doc.children, None, &sheet, &mut removed_ids);
    if !removed_ids.is_empty() {
        // remove <use> elements referencing removed ids
        let removed: Vec<String> = removed_ids.clone();
        prune_uses(&mut doc.children, &removed);
    }
}

fn prune_uses(nodes: &mut Vec<Node>, removed: &[String]) {
    nodes.retain(|n| {
        if let Node::Element(el) = n {
            if el.name == "use" {
                for (k, v) in &el.attrs {
                    for id in find_references(k, v) {
                        if removed.contains(&id) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    });
    for n in nodes.iter_mut() {
        if let Node::Element(el) = n {
            prune_uses(&mut el.children, removed);
        }
    }
}

fn prune_containers(
    nodes: &mut Vec<Node>,
    parent_name: Option<&str>,
    sheet: &Stylesheet,
    removed_ids: &mut Vec<String>,
) {
    // exit order: recurse first
    for n in nodes.iter_mut() {
        if let Node::Element(el) = n {
            let name = el.name.clone();
            prune_containers(&mut el.children, Some(&name), sheet, removed_ids);
        }
    }
    nodes.retain(|n| {
        let el = match n {
            Node::Element(el) => el,
            _ => return true,
        };
        if el.name == "svg" || !CONTAINER.contains(&el.name.as_str()) {
            return true;
        }
        if !significant(&el.children).is_empty() {
            return true;
        }
        if el.name == "pattern" && !el.attrs.is_empty() {
            return true;
        }
        if el.name == "mask" && el.has("id") {
            return true;
        }
        if parent_name == Some("switch") {
            return true;
        }
        if el.name == "g" && (el.has("filter") || has_filter(sheet, el)) {
            return true;
        }
        if let Some(id) = el.get("id") {
            removed_ids.push(id.to_string());
        }
        false
    });
}

// ---------- collapseGroups ----------
fn has_animated_attr(el: &Element, name: &str) -> bool {
    if ANIMATION.contains(&el.name.as_str()) && el.get("attributeName") == Some(name) {
        return true;
    }
    for c in &el.children {
        if let Node::Element(child) = c {
            if has_animated_attr(child, name) {
                return true;
            }
        }
    }
    false
}

fn collapse_groups(doc: &mut Document) {
    let sheet = collect_stylesheet(doc);
    collapse_children(&mut doc.children, true, None, &sheet);
}

fn collapse_children(
    nodes: &mut Vec<Node>,
    parent_is_root: bool,
    parent_name: Option<&str>,
    sheet: &Stylesheet,
) {
    let mut i = 0;
    while i < nodes.len() {
        // recurse (exit order for descendants first)
        let child_name = match &nodes[i] {
            Node::Element(el) => Some(el.name.clone()),
            _ => None,
        };
        if let Some(cn) = child_name {
            if let Node::Element(el) = &mut nodes[i] {
                collapse_children(&mut el.children, false, Some(&cn), sheet);
            }
        }
        // now apply to nodes[i] itself
        let mut splice: Option<Vec<Node>> = None;
        if let Node::Element(el) = &mut nodes[i] {
            if !parent_is_root && parent_name != Some("switch") && el.name == "g" {
                let sig = significant(&el.children);
                if !sig.is_empty() {
                    // move attrs to single child
                    if !el.attrs.is_empty() && sig.len() == 1 {
                        if let Node::Element(first) = &sig[0] {
                            let node_has_filter = el.has("filter") || has_filter(sheet, el);
                            let cond = first.get("id").is_none()
                                && !node_has_filter
                                && (el.get("class").is_none() || first.get("class").is_none())
                                && ((el.get("clip-path").is_none() && el.get("mask").is_none())
                                    || (first.name == "g"
                                        && el.get("transform").is_none()
                                        && first.get("transform").is_none()));
                            if cond {
                                let mut new_attrs = first.attrs.clone();
                                let mut abort = false;
                                for (name, value) in el.attrs.clone().iter() {
                                    if has_animated_attr(first, name) {
                                        abort = true;
                                        break;
                                    }
                                    if let Some(slot) =
                                        new_attrs.iter_mut().find(|(k, _)| k == name)
                                    {
                                        if name == "transform" {
                                            slot.1 = format!("{} {}", value, slot.1);
                                        } else if slot.1 == "inherit" {
                                            slot.1 = value.clone();
                                        } else if !INHERITABLE.contains(&name.as_str())
                                            && slot.1 != *value
                                        {
                                            abort = true;
                                            break;
                                        }
                                    } else {
                                        new_attrs.push((name.clone(), value.clone()));
                                    }
                                }
                                if !abort {
                                    el.attrs.clear();
                                    // write back to the real first significant child
                                    for c in el.children.iter_mut() {
                                        if let Node::Element(fc) = c {
                                            fc.attrs = new_attrs.clone();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if el.attrs.is_empty() {
                        // recompute: attr-moving above mutated the real children,
                        // while `sig` is a stale clone taken before it
                        let sig2 = significant(&el.children);
                        let has_anim = sig2.iter().any(|c| {
                            matches!(c, Node::Element(e) if ANIMATION.contains(&e.name.as_str()))
                        });
                        if !has_anim {
                            splice = Some(sig2);
                        }
                    }
                }
            }
        }
        if let Some(children) = splice {
            let removed = nodes.remove(i);
            let _ = removed;
            for (offset, child) in children.into_iter().enumerate() {
                nodes.insert(i + offset, child);
            }
            // do not advance: spliced children were already processed as descendants,
            // but re-checking is harmless; advance by 1 to avoid infinite loops
            i += 1;
        } else {
            i += 1;
        }
    }
}

// ---------- convertTransform ----------
fn float_digits(n: f64) -> i32 {
    let s = crate::tools::fmt_num(n);
    match s.find('.') {
        Some(idx) => (s.len() - idx - 1) as i32,
        None => 0,
    }
}

fn define_precision(data: &[TransformItem], params: &TransformParams) -> TransformParams {
    let mut p = params.clone();
    let mut matrix_data: Vec<f64> = Vec::new();
    for item in data {
        if item.name == "matrix" {
            matrix_data.extend(item.data.iter().take(4).cloned());
        }
    }
    let mut number_of_digits = p.transform_precision;
    if !matrix_data.is_empty() {
        let max_digits = matrix_data.iter().map(|n| float_digits(*n)).max().unwrap_or(0);
        p.transform_precision = p.transform_precision.min(if max_digits == 0 && false {
            p.transform_precision
        } else {
            max_digits
        });
        // JS: Math.max(...floatDigits) || transformPrecision  (0 is falsy)
        if max_digits == 0 {
            // restore: min(orig, orig) = orig; already min'd with 0 would be wrong
            p.transform_precision = params.transform_precision;
        }
        number_of_digits = matrix_data
            .iter()
            .map(|n| {
                crate::tools::fmt_num(*n)
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .count() as i32
            })
            .max()
            .unwrap_or(number_of_digits);
    }
    if p.deg_precision.is_none() {
        p.deg_precision = Some(0.max(p.float_precision.min(number_of_digits - 2)));
    }
    p
}

fn convert_one_transform(el: &mut Element, attr: &str, params_in: &TransformParams) {
    let value = match el.get(attr) {
        Some(v) => v.to_string(),
        None => return,
    };
    let mut data = transform2js(&value);
    let params = define_precision(&data, params_in);
    if params.collapse_into_one && data.len() > 1 {
        data = vec![transforms_multiply(&data)];
    }
    if params.convert_to_shorts {
        data = convert_to_shorts(data, &params);
    } else {
        for item in data.iter_mut() {
            round_transform(item, &params);
        }
    }
    if params.remove_useless {
        data.retain(|t| !is_useless_transform(t));
    }
    if data.is_empty() {
        el.remove_attr(attr);
    } else {
        let s = js2transform(&data, &params);
        el.set(attr, s);
    }
}

fn is_useless_transform(t: &TransformItem) -> bool {
    let d = &t.data;
    if d.is_empty() {
        return false;
    }
    if ["translate", "rotate", "skewX", "skewY"].contains(&t.name.as_str())
        && (d.len() == 1 || t.name == "rotate")
        && d[0] == 0.0
    {
        return true;
    }
    if t.name == "translate" && d[0] == 0.0 && d.get(1).map(|v| *v == 0.0).unwrap_or(true) {
        return true;
    }
    if t.name == "scale" && d[0] == 1.0 && (d.len() < 2 || d[1] == 1.0) {
        return true;
    }
    if t.name == "matrix"
        && d.len() >= 6
        && d[0] == 1.0
        && d[3] == 1.0
        && d[1] == 0.0
        && d[2] == 0.0
        && d[4] == 0.0
        && d[5] == 0.0
    {
        return true;
    }
    false
}

fn convert_to_shorts(mut transforms: Vec<TransformItem>, params: &TransformParams) -> Vec<TransformItem> {
    let mut i = 0usize;
    while i < transforms.len() {
        let mut transform = transforms[i].clone();
        if params.matrix_to_transform && transform.name == "matrix" {
            let decomposed = matrix_to_transform(&transform, params);
            if js2transform(&decomposed, params).len() <= js2transform(&[transform.clone()], params).len() {
                transforms.splice(i..i + 1, decomposed);
            }
            transform = transforms[i].clone();
        }
        round_transform(&mut transform, params);
        transforms[i] = transform.clone();
        if params.short_translate
            && transform.name == "translate"
            && transform.data.len() == 2
            && transform.data[1] == 0.0
        {
            transforms[i].data.pop();
            transform = transforms[i].clone();
        }
        if params.short_scale
            && transform.name == "scale"
            && transform.data.len() == 2
            && transform.data[0] == transform.data[1]
        {
            transforms[i].data.pop();
            transform = transforms[i].clone();
        }
        let _ = transform;
        if params.short_rotate && i >= 2 {
            let a = transforms[i - 2].clone();
            let b = transforms[i - 1].clone();
            let c = transforms[i].clone();
            if a.name == "translate"
                && b.name == "rotate"
                && c.name == "translate"
                && a.data.len() >= 2
                && c.data.len() >= 2
                && a.data[0] == -c.data[0]
                && a.data[1] == -c.data[1]
            {
                let merged = TransformItem {
                    name: "rotate".into(),
                    data: vec![b.data[0], a.data[0], a.data[1]],
                };
                transforms.splice(i - 2..i + 1, [merged]);
                i = i.saturating_sub(2);
                continue;
            }
        }
        i += 1;
    }
    transforms
}

fn convert_transform_plugin(doc: &mut Document, params: &Json) {
    let p = TransformParams {
        convert_to_shorts: params.bool_param("convertToShorts", true),
        deg_precision: params.get("degPrecision").and_then(|v| {
            if let Json::Num(n) = v { Some(*n as i32) } else { None }
        }),
        float_precision: params.num_param("floatPrecision", 3.0) as i32,
        transform_precision: params.num_param("transformPrecision", 5.0) as i32,
        matrix_to_transform: params.bool_param("matrixToTransform", true),
        short_translate: params.bool_param("shortTranslate", true),
        short_scale: params.bool_param("shortScale", true),
        short_rotate: params.bool_param("shortRotate", true),
        remove_useless: params.bool_param("removeUseless", true),
        collapse_into_one: params.bool_param("collapseIntoOne", true),
        leading_zero: params.bool_param("leadingZero", true),
        negative_extra_space: params.bool_param("negativeExtraSpace", false),
    };
    let mut els: Vec<*mut Element> = Vec::new();
    collect_element_ptrs(&mut doc.children, &mut els);
    for ptr in els {
        unsafe {
            convert_one_transform(&mut *ptr, "transform", &p);
            convert_one_transform(&mut *ptr, "gradientTransform", &p);
            convert_one_transform(&mut *ptr, "patternTransform", &p);
        }
    }
}

fn collect_element_ptrs(nodes: &mut [Node], out: &mut Vec<*mut Element>) {
    for n in nodes.iter_mut() {
        if let Node::Element(el) = n {
            out.push(el as *mut Element);
            collect_element_ptrs(&mut el.children, out);
        }
    }
}

// ---------- mergePaths ----------
fn element_has_url(sheet: &Stylesheet, el: &Element, att: &str) -> bool {
    match compute_prop(sheet, el, att) {
        Some(v) => includes_url_reference(&v),
        None => false,
    }
}

fn merge_paths(doc: &mut Document, params: &Json) {
    let force = params.bool_param("force", false);
    let float_precision = params.num_param("floatPrecision", 3.0) as i32;
    let no_space_after_flags = params.bool_param("noSpaceAfterFlags", false);
    let sheet = collect_stylesheet(doc);
    merge_paths_children(&mut doc.children, &sheet, force, Some(float_precision), no_space_after_flags, false);
}

fn merge_paths_children(
    nodes: &mut Vec<Node>,
    sheet: &Stylesheet,
    force: bool,
    float_precision: Option<i32>,
    no_space_after_flags: bool,
    parent_is_text: bool,
) {
    // JS mergePaths never removes text nodes. Dropping whitespace-only
    // text only mimics the JS parser, which drops it outside text
    // elements but preserves it verbatim inside them (see
    // `lib/parser.js` ontext), so keep it for text-element parents.
    if !parent_is_text {
        *nodes = significant(nodes);
    }
    merge_in_parent(nodes, sheet, force, float_precision, no_space_after_flags);
    for n in nodes.iter_mut() {
        if let Node::Element(el) = n {
            let is_text = is_text_elem(&el.name);
            merge_paths_children(&mut el.children, sheet, force, float_precision, no_space_after_flags, is_text);
        }
    }
}

fn merge_in_parent(
    children: &mut Vec<Node>,
    sheet: &Stylesheet,
    force: bool,
    float_precision: Option<i32>,
    no_space_after_flags: bool,
) {
    // Work on significant children indices mapping to real indices.
    // JS iterates node.children including text nodes; text breaks adjacency similarly.
    if children.len() <= 1 {
        return;
    }
    let mut to_remove: Vec<usize> = Vec::new();
    let mut prev_idx = 0usize;
    let mut prev_path_data: Option<Vec<PathItem>> = None;

    let is_path = |n: &Node| -> Option<Element> {
        if let Node::Element(el) = n {
            if el.name == "path" && significant(&el.children).is_empty() && el.has("d") {
                return Some(el.clone());
            }
        }
        None
    };

    for i in 1..children.len() {
        let prev_path = is_path(&children[prev_idx]);
        if prev_path.is_none() {
            if let Some(pd) = prev_path_data.take() {
                if let Node::Element(el) = &mut children[prev_idx] {
                    let s = js2path(&pd, float_precision, no_space_after_flags);
                    el.set("d", s);
                }
            }
            prev_idx = i;
            continue;
        }
        let child_path = is_path(&children[i]);
        if child_path.is_none() {
            if let Some(pd) = prev_path_data.take() {
                if let Node::Element(el) = &mut children[prev_idx] {
                    let s = js2path(&pd, float_precision, no_space_after_flags);
                    el.set("d", s);
                }
            }
            prev_idx = i;
            continue;
        }
        let child_el = child_path.unwrap();
        let prev_el = prev_path.unwrap();
        // computed style guards on child
        let blocked = ["marker-start", "marker-mid", "marker-end", "clip-path", "mask", "mask-image"]
            .iter()
            .any(|p| compute_prop(sheet, &child_el, p).is_some())
            || ["fill", "filter", "stroke"]
                .iter()
                .any(|p| element_has_url(sheet, &child_el, p));
        if blocked {
            if let Some(pd) = prev_path_data.take() {
                if let Node::Element(el) = &mut children[prev_idx] {
                    let s = js2path(&pd, float_precision, no_space_after_flags);
                    el.set("d", s);
                }
            }
            prev_idx = i;
            continue;
        }
        if child_el.attrs.len() != prev_el.attrs.len() {
            if let Some(pd) = prev_path_data.take() {
                if let Node::Element(el) = &mut children[prev_idx] {
                    let s = js2path(&pd, float_precision, no_space_after_flags);
                    el.set("d", s);
                }
            }
            prev_idx = i;
            continue;
        }
        let attrs_differ = child_el.attrs.iter().any(|(k, v)| {
            k != "d" && prev_el.get(k) != Some(v.as_str())
        });
        if attrs_differ {
            if let Some(pd) = prev_path_data.take() {
                if let Node::Element(el) = &mut children[prev_idx] {
                    let s = js2path(&pd, float_precision, no_space_after_flags);
                    el.set("d", s);
                }
            }
            prev_idx = i;
            continue;
        }
        let has_prev = prev_path_data.is_some();
        let current = path2js(child_el.get("d").unwrap_or(""));
        if prev_path_data.is_none() {
            prev_path_data = Some(path2js(prev_el.get("d").unwrap_or("")));
        }
        let prev_pd = prev_path_data.clone().unwrap();
        if force || !intersects(&prev_pd, &current) {
            prev_path_data.as_mut().unwrap().extend(current);
            to_remove.push(i);
            continue;
        }
        if has_prev {
            if let Some(pd) = prev_path_data.take() {
                if let Node::Element(el) = &mut children[prev_idx] {
                    let s = js2path(&pd, float_precision, no_space_after_flags);
                    el.set("d", s);
                }
            }
        }
        prev_idx = i;
        prev_path_data = None;
    }
    if let Some(pd) = prev_path_data.take() {
        if let Node::Element(el) = &mut children[prev_idx] {
            let s = js2path(&pd, float_precision, no_space_after_flags);
            el.set("d", s);
        }
    }
    if !to_remove.is_empty() {
        let mut idx = 0;
        children.retain(|_| {
            let keep = !to_remove.contains(&idx);
            idx += 1;
            keep
        });
    }
}

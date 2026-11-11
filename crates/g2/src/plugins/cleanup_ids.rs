//! Port of `plugins/cleanupIds.js`.

use crate::dom::{element_by_uid_mut, Node};
use crate::json::Json;
use crate::tools::{encode_uri, find_references, has_scripts};
use std::collections::{HashMap, HashSet};

const ID_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

fn generate_id(current: &mut Option<Vec<usize>>) -> Vec<usize> {
    let max = ID_CHARS.len() - 1;
    match current {
        None => {
            *current = Some(vec![0]);
            vec![0]
        }
        Some(cur) => {
            let last = cur.len() - 1;
            cur[last] += 1;
            for i in (1..cur.len()).rev() {
                if cur[i] > max {
                    cur[i] = 0;
                    cur[i - 1] += 1;
                }
            }
            if cur[0] > max {
                cur[0] = 0;
                cur.insert(0, 0);
            }
            cur.clone()
        }
    }
}

fn id_string(arr: &[usize]) -> String {
    arr.iter().map(|&i| ID_CHARS[i] as char).collect()
}

struct Ref {
    element_uid: u64,
    attr: String,
}

#[allow(clippy::ptr_arg)]
pub fn apply(root: &mut Vec<Node>, params: &Json) {
    let remove = params.get("remove").map(|v| v.as_bool(true)).unwrap_or(true);
    let minify = params.get("minify").map(|v| v.as_bool(true)).unwrap_or(true);
    let force = params.get("force").map(|v| v.as_bool(false)).unwrap_or(false);
    let preserve_ids: HashSet<String> = params
        .get("preserve")
        .map(|v| v.as_str_list().into_iter().collect())
        .unwrap_or_default();
    let preserve_prefixes: Vec<String> =
        params.get("preservePrefixes").map(|v| v.as_str_list()).unwrap_or_default();

    let is_preserved = |id: &str| -> bool {
        preserve_ids.contains(id) || preserve_prefixes.iter().any(|p| id.starts_with(p.as_str()))
    };

    // Collect in document order.
    let mut node_by_id_order: Vec<String> = Vec::new();
    let mut node_by_id: HashMap<String, u64> = HashMap::new();
    let mut references_order: Vec<String> = Vec::new();
    let mut references_by_id: HashMap<String, Vec<Ref>> = HashMap::new();
    let mut deoptimized = false;

    // Gather element snapshots (uid, name, attrs, has_defs_only for svg, child count/style).
    struct Snap {
        uid: u64,
        attrs: Vec<(String, String)>,
        child_count: usize,
        svg_defs_only: bool,
    }
    let mut snaps: Vec<Snap> = Vec::new();
    fn walk(nodes: &[Node], snaps: &mut Vec<Snap>, force: bool, deopt: &mut bool) {
        for n in nodes {
            if let Node::Element(e) = n {
                // svg defs-only skip (when !force): do not descend
                let defs_only = e.name == "svg"
                    && e.children.iter().all(|c| matches!(c, Node::Element(ch) if ch.name == "defs"));
                if !force && e.name == "svg" && defs_only {
                    snaps.push(Snap {
                        uid: e.uid,
                        attrs: e.attrs.clone(),
                        child_count: e.children.len(),
                        svg_defs_only: true,
                    });
                    continue; // visitSkip
                }
                if !force
                    && ((e.name == "style" && !e.children.is_empty()) || has_scripts(e))
                {
                    *deopt = true;
                    // JS returns from enter but still visits children; fall through
                }
                snaps.push(Snap {
                    uid: e.uid,
                    attrs: e.attrs.clone(),
                    child_count: e.children.len(),
                    svg_defs_only: false,
                });
                walk(&e.children, snaps, force, deopt);
            }
        }
    }
    walk(root, &mut snaps, force, &mut deoptimized);
    let _ = snaps.iter().map(|s| s.child_count).sum::<usize>();

    for snap in &snaps {
        if snap.svg_defs_only && !force {
            continue; // attributes of svg itself were not processed in JS? (visitSkip skips subtree; svg enter returned visitSkip BEFORE attr loop? No: check code: svg branch returns visitSkip inside enter, before attribute loop, so its own attrs also skipped. Our `continue` here matches, but note snap was pushed; skipping here is the attr skip.)
        }
        for (name, value) in &snap.attrs {
            if name == "id" {
                if node_by_id.contains_key(value) {
                    // remove repeated id immediately
                    if let Some(e) = element_by_uid_mut(root, snap.uid) {
                        e.remove_attr("id");
                    }
                } else {
                    node_by_id.insert(value.clone(), snap.uid);
                    node_by_id_order.push(value.clone());
                }
            } else {
                for id in find_references(name, value) {
                    if !references_by_id.contains_key(&id) {
                        references_order.push(id.clone());
                    }
                    references_by_id
                        .entry(id)
                        .or_default()
                        .push(Ref { element_uid: snap.uid, attr: name.clone() });
                }
            }
        }
    }

    if deoptimized {
        return;
    }

    let mut current_id: Option<Vec<usize>> = None;
    for id in &references_order {
        let Some(&node_uid) = node_by_id.get(id) else { continue };
        if minify && !is_preserved(id) {
            let new_id = loop {
                let cand = id_string(&generate_id(&mut current_id));
                let conflict = references_by_id.contains_key(&cand)
                    && !node_by_id.contains_key(&cand);
                if !is_preserved(&cand) && !conflict {
                    break cand;
                }
            };
            if let Some(e) = element_by_uid_mut(root, node_uid) {
                e.set("id", &new_id);
            }
            if let Some(refs) = references_by_id.get(id) {
                for r in refs {
                    let uid = r.element_uid;
                    let attr = r.attr.clone();
                    if let Some(e) = element_by_uid_mut(root, uid) {
                        if let Some(value) = e.get(&attr).map(|s| s.to_string()) {
                            let new_value = if value.contains('#') {
                                let encoded = format!("#{}", encode_uri(id));
                                let step = replace_first(&value, &encoded, &format!("#{new_id}"));
                                replace_first(&step, &format!("#{id}"), &format!("#{new_id}"))
                            } else {
                                replace_first(&value, &format!("{id}."), &format!("{new_id}."))
                            };
                            e.set(&attr, &new_value);
                        }
                    }
                }
            }
        }
        node_by_id.remove(id);
    }

    if remove {
        for id in &node_by_id_order {
            if let Some(&uid) = node_by_id.get(id) {
                if !is_preserved(id) {
                    if let Some(e) = element_by_uid_mut(root, uid) {
                        e.remove_attr("id");
                    }
                }
            }
        }
    }
}

fn replace_first(haystack: &str, needle: &str, replacement: &str) -> String {
    haystack.replacen(needle, replacement, 1)
}

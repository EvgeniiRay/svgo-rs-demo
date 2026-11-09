//! Port of `plugins/mergeStyles.js`.

use crate::dom::{detach_by_uid, element_by_uid_mut, Node};

pub fn apply(root: &mut Vec<Node>) {
    // Collect candidate style elements in document order (skip foreignObject).
    let mut uids: Vec<u64> = Vec::new();
    fn walk(nodes: &[Node], uids: &mut Vec<u64>) {
        for n in nodes {
            if let Node::Element(e) = n {
                if e.name == "foreignObject" {
                    continue;
                }
                if e.name == "style" {
                    uids.push(e.uid);
                }
                walk(&e.children, uids);
            }
        }
    }
    walk(root, &mut uids);

    let mut first_uid: Option<u64> = None;
    let mut collected = String::new();
    let mut cdata_mode = false;

    for uid in uids {
        // Extract info first (immutable), then mutate.
        let (valid_type, css, has_cdata, media) = {
            let Some(e) = crate::dom::element_by_uid(root, uid) else { continue };
            let valid = match e.get("type") {
                None => true,
                Some(t) => t.is_empty() || t == "text/css",
            };
            let mut css = String::new();
            let mut has_cdata = false;
            for child in &e.children {
                match child {
                    Node::Text(v) => css.push_str(v),
                    Node::Cdata(v) => {
                        has_cdata = true;
                        css.push_str(v);
                    }
                    _ => {}
                }
            }
            (valid, css, has_cdata, e.get("media").map(|s| s.to_string()))
        };
        if !valid_type {
            continue;
        }
        if has_cdata {
            cdata_mode = true;
        }
        if css.trim().is_empty() {
            detach_by_uid(root, uid);
            continue;
        }
        if let Some(media) = &media {
            collected.push_str(&format!("@media {media}{{{css}}}"));
            if let Some(e) = element_by_uid_mut(root, uid) {
                e.remove_attr("media");
            }
        } else {
            collected.push_str(&css);
        }
        match first_uid {
            None => first_uid = Some(uid),
            Some(first) => {
                detach_by_uid(root, uid);
                if let Some(e) = element_by_uid_mut(root, first) {
                    let node = if cdata_mode {
                        Node::Cdata(collected.clone())
                    } else {
                        Node::Text(collected.clone())
                    };
                    e.children = vec![node];
                }
            }
        }
    }
}

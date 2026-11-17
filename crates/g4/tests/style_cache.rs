//! Port of the `lib/style.test.js` case added by upstream
//! svg/svgo@68c0660: inline declarations are cached by style value, and
//! updating the attribute cannot return stale declarations.

use svgo_rs_g4::parser::parse_svg;
use svgo_rs_g4::style::{collect_stylesheet, compute_style, Computed};
use svgo_rs_g4::xast::{Element, Node};

fn find_by_id<'a>(children: &'a [Node], id: &str) -> Option<&'a Element> {
    for child in children {
        if let Node::Element(el) = child {
            if el.get("id") == Some(id) {
                return Some(el);
            }
            if let Some(found) = find_by_id(&el.children, id) {
                return Some(found);
            }
        }
    }
    None
}

#[test]
fn caches_inline_declarations_by_style_value() {
    let mut root = parse_svg(
        "<svg><rect id=\"one\" style=\"fill: red\" /><rect id=\"two\" style=\"fill: red\" /></svg>",
    );
    let stylesheet = collect_stylesheet(&root);
    let one = find_by_id(&root.children, "one").unwrap().clone();
    let two = find_by_id(&root.children, "two").unwrap().clone();

    assert_eq!(
        compute_style(&stylesheet, &one, &[]).get("fill"),
        Some(&Computed::Static("red".to_string()))
    );
    assert_eq!(
        compute_style(&stylesheet, &two, &[]).get("fill"),
        Some(&Computed::Static("red".to_string()))
    );
    assert_eq!(stylesheet.declaration_cache.borrow().len(), 1);

    // a changed style attribute is a different cache key, never stale
    let mut two = two;
    two.set("style", "fill: blue");
    let _ = &mut root;
    assert_eq!(
        compute_style(&stylesheet, &two, &[]).get("fill"),
        Some(&Computed::Static("blue".to_string()))
    );
    assert_eq!(stylesheet.declaration_cache.borrow().len(), 2);
}

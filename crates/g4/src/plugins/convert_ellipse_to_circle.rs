use crate::plugins::Plugin;
use crate::xast::{Node, Root};

pub struct ConvertEllipseToCircle;
impl Plugin for ConvertEllipseToCircle {
    fn name(&self) -> &'static str { "convertEllipseToCircle" }
    fn apply(&self, root: &mut Root) { walk(&mut root.children); }
}
fn walk(children: &mut [Node]) {
    for c in children {
        if let Node::Element(e) = c {
            if e.name == "ellipse" {
                let rx = e.get("rx").unwrap_or("0").to_string();
                let ry = e.get("ry").unwrap_or("0").to_string();
                if rx == ry || rx == "auto" || ry == "auto" {
                    e.name = "circle".to_string();
                    let radius = if rx == "auto" { ry.clone() } else { rx.clone() };
                    e.remove_attr("rx"); e.remove_attr("ry"); e.set("r", &radius);
                }
            }
            walk(&mut e.children);
        }
    }
}

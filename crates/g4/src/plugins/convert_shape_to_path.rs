use crate::path_data::{stringify_path_data, PathItem};
use crate::plugins::{js_number, Plugin};
use crate::xast::{Node, Root};

#[derive(Debug, Clone, Copy, Default)]
pub struct ConvertShapeToPathParams { pub convert_arcs: bool, pub float_precision: Option<i32> }

#[derive(Default)]
pub struct ConvertShapeToPath { pub params: ConvertShapeToPathParams }
impl Plugin for ConvertShapeToPath {
    fn name(&self) -> &'static str { "convertShapeToPath" }
    fn apply(&self, root: &mut Root) { let p = self.params; walk(&mut root.children, &p); }
}
fn extract_numbers(s: &str) -> Vec<f64> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new(); let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let startish = c.is_ascii_digit() || c=='.' || ((c=='+'||c=='-') && i+1<chars.len() && (chars[i+1].is_ascii_digit()||chars[i+1]=='.'));
        if !startish { i+=1; continue; }
        let start = i;
        if chars[i]=='+'||chars[i]=='-' { i+=1; }
        let mut has_digits = false; let mut has_dot = false;
        while i < chars.len() {
            if chars[i].is_ascii_digit() { has_digits=true; i+=1; }
            else if chars[i]=='.' && !has_dot { has_dot=true; i+=1; }
            else { break; }
        }
        if !has_digits { continue; }
        if i < chars.len() && (chars[i]=='e'||chars[i]=='E') {
            let mut j = i+1;
            if j<chars.len() && (chars[j]=='+'||chars[j]=='-') { j+=1; }
            if j<chars.len() && chars[j].is_ascii_digit() { while j<chars.len() && chars[j].is_ascii_digit() { j+=1; } i=j; }
        }
        let tok: String = chars[start..i].iter().collect();
        if let Ok(v) = tok.parse::<f64>() { out.push(v); }
    }
    out
}
fn walk(children: &mut Vec<Node>, p: &ConvertShapeToPathParams) {
    let mut i = 0;
    while i < children.len() {
        let mut detach = false;
        if let Node::Element(e) = &mut children[i] {
            // rect
            if e.name=="rect" && e.has("width") && e.has("height") && !e.has("rx") && !e.has("ry") {
                let x = js_number(e.get("x").unwrap_or("0")); let y = js_number(e.get("y").unwrap_or("0"));
                let w = js_number(e.get("width").unwrap()); let h = js_number(e.get("height").unwrap());
                if !((x - y + w - h).is_nan()) {
                    let pd = vec![PathItem{command:'M',args:vec![x,y]},PathItem{command:'H',args:vec![x+w]},PathItem{command:'V',args:vec![y+h]},PathItem{command:'H',args:vec![x]},PathItem{command:'z',args:vec![]}];
                    e.name="path".to_string(); e.set("d", &stringify_path_data(&pd, p.float_precision));
                    e.remove_attr("x"); e.remove_attr("y"); e.remove_attr("width"); e.remove_attr("height");
                }
            }
            if e.name=="line" {
                let x1=js_number(e.get("x1").unwrap_or("0")); let y1=js_number(e.get("y1").unwrap_or("0"));
                let x2=js_number(e.get("x2").unwrap_or("0")); let y2=js_number(e.get("y2").unwrap_or("0"));
                if !(x1-y1+x2-y2).is_nan() {
                    let pd=vec![PathItem{command:'M',args:vec![x1,y1]},PathItem{command:'L',args:vec![x2,y2]}];
                    e.name="path".to_string(); e.set("d", &stringify_path_data(&pd, p.float_precision));
                    e.remove_attr("x1"); e.remove_attr("y1"); e.remove_attr("x2"); e.remove_attr("y2");
                }
            }
            if (e.name=="polyline"||e.name=="polygon") && e.has("points") {
                let coords = extract_numbers(e.get("points").unwrap());
                if coords.len() < 4 { detach = true; }
                else {
                    let is_polygon = e.name=="polygon";
                    let mut pd: Vec<PathItem> = Vec::new();
                    for (idx, pair) in coords.chunks(2).enumerate() { pd.push(PathItem{command: if idx==0 {'M'} else {'L'}, args: pair.to_vec()}); }
                    if is_polygon { pd.push(PathItem{command:'z',args:vec![]}); }
                    e.name="path".to_string(); e.set("d", &stringify_path_data(&pd, p.float_precision)); e.remove_attr("points");
                }
            }
            if e.name=="circle" && p.convert_arcs {
                let cx=js_number(e.get("cx").unwrap_or("0")); let cy=js_number(e.get("cy").unwrap_or("0")); let r=js_number(e.get("r").unwrap_or("0"));
                if !(cx-cy+r).is_nan() {
                    let pd=vec![PathItem{command:'M',args:vec![cx,cy-r]},PathItem{command:'A',args:vec![r,r,0.0,1.0,0.0,cx,cy+r]},PathItem{command:'A',args:vec![r,r,0.0,1.0,0.0,cx,cy-r]},PathItem{command:'z',args:vec![]}];
                    e.name="path".to_string(); e.set("d", &stringify_path_data(&pd, p.float_precision));
                    e.remove_attr("cx"); e.remove_attr("cy"); e.remove_attr("r");
                }
            }
            if e.name=="ellipse" && p.convert_arcs {
                let cx=js_number(e.get("cx").unwrap_or("0")); let cy=js_number(e.get("cy").unwrap_or("0"));
                let rx=js_number(e.get("rx").unwrap_or("0")); let ry=js_number(e.get("ry").unwrap_or("0"));
                if !(cx-cy+rx-ry).is_nan() {
                    let pd=vec![PathItem{command:'M',args:vec![cx,cy-ry]},PathItem{command:'A',args:vec![rx,ry,0.0,1.0,0.0,cx,cy+ry]},PathItem{command:'A',args:vec![rx,ry,0.0,1.0,0.0,cx,cy-ry]},PathItem{command:'z',args:vec![]}];
                    e.name="path".to_string(); e.set("d", &stringify_path_data(&pd, p.float_precision));
                    e.remove_attr("cx"); e.remove_attr("cy"); e.remove_attr("rx"); e.remove_attr("ry");
                }
            }
        }
        if detach { children.remove(i); continue; }
        if let Node::Element(e) = &mut children[i] { walk(&mut e.children, p); }
        i+=1;
    }
}

//! XAST DOM mirroring SVGO's lib/types.js: root, element, text, cdata,
//! comment, doctype and instruction nodes. Attribute order is preserved
//! (insertion order, as with JS objects); setting an existing attribute
//! keeps its position, setting a new one appends it.

#[derive(Debug, Clone, PartialEq)]
pub enum Child {
    Element(Element),
    Text(String),
    Cdata(String),
    Comment(String),
    Doctype { name: String, data: String },
    Instruction { name: String, value: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Child>,
}

impl Element {
    pub fn new(name: &str) -> Self {
        Element { name: name.to_string(), attributes: Vec::new(), children: Vec::new() }
    }
    pub fn get(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
    pub fn has(&self, name: &str) -> bool {
        self.attributes.iter().any(|(n, _)| n == name)
    }
    pub fn set(&mut self, name: &str, value: &str) {
        if let Some(slot) = self.attributes.iter_mut().find(|(n, _)| n == name) {
            slot.1 = value.to_string();
        } else {
            self.attributes.push((name.to_string(), value.to_string()));
        }
    }
    pub fn remove(&mut self, name: &str) {
        self.attributes.retain(|(n, _)| n != name);
    }
    pub fn attr_names(&self) -> Vec<String> {
        self.attributes.iter().map(|(n, _)| n.clone()).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Root {
    pub children: Vec<Child>,
}

/// Pre-order walk of every element (enter only), mirroring lib/util/visit.js
/// for visitors that never skip or detach.
pub fn walk_elements<F: FnMut(&mut Element)>(children: &mut [Child], f: &mut F) {
    for child in children {
        if let Child::Element(el) = child {
            f(el);
            walk_elements(&mut el.children, f);
        }
    }
}

/// Collect element names matching `pred` anywhere in the tree.
pub fn any_element<F: Fn(&Element) -> bool>(children: &[Child], pred: &F) -> bool {
    children.iter().any(|c| match c {
        Child::Element(el) => pred(el) || any_element(&el.children, pred),
        _ => false,
    })
}

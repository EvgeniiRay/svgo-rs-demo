//! XAST types mirroring `reference/svgo/lib/types.ts`.

#[derive(Debug, Clone, PartialEq)]
pub enum XastChild {
    Doctype { name: String, doctype: String },
    Instruction { name: String, value: String },
    Comment { value: String },
    Cdata { value: String },
    Text { value: String },
    Element(XastElement),
}

#[derive(Debug, Clone, PartialEq)]
pub struct XastElement {
    pub name: String,
    /// Attributes in document order (JS `Record` insertion order).
    pub attributes: Vec<(String, String)>,
    pub children: Vec<XastChild>,
}

impl XastElement {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), attributes: Vec::new(), children: Vec::new() }
    }
    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
    pub fn set_attr(&mut self, name: &str, value: String) {
        if let Some(slot) = self.attributes.iter_mut().find(|(n, _)| n == name) {
            slot.1 = value;
        } else {
            self.attributes.push((name.to_string(), value));
        }
    }
    pub fn remove_attr(&mut self, name: &str) {
        self.attributes.retain(|(n, _)| n != name);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct XastRoot {
    pub children: Vec<XastChild>,
}

/// Mirror of `detachNodeFromParent` in `lib/xast.js`.
pub fn detach_node_from_parent(parent_children: &mut Vec<XastChild>, index: usize) {
    parent_children.remove(index);
}

/// Elements whose text is preserved verbatim by the parser/stringifier
/// (`textElems` in `plugins/_collections.js`: textContent group + pre + title).
pub fn is_text_elem(name: &str) -> bool {
    matches!(
        name,
        "a" | "altGlyph" | "altGlyphDef" | "altGlyphItem" | "glyph" | "glyphRef" | "text"
            | "textPath" | "tref" | "tspan" | "pre" | "title"
    )
}

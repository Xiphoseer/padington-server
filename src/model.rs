//! # The document model
//!
//! This module is derived from the `prosemirror-markdown` schema and the
//! the general JSON serialization of nodes.

use serde::Serialize;

#[derive(Debug, Copy, Clone, Serialize)]
#[serde(into = "&'static str")]
pub enum NodeType {
    Doc,
}

impl From<NodeType> for &'static str {
    fn from(nt: NodeType) -> &'static str {
        match nt {
            NodeType::Doc => "doc",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadingAttrs {
    pub level: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodeBlockAttrs {
    pub params: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    Doc {
        content: Vec<Node>,
    },
    Heading {
        attrs: HeadingAttrs,
        content: Vec<Node>,
    },
    CodeBlock {
        attrs: CodeBlockAttrs,
        content: Vec<Node>,
    },
    Text {
        text: String,
    },
    Blockquote {
        content: Vec<Node>,
    },
    Paragraph {
        content: Vec<Node>,
    },
}

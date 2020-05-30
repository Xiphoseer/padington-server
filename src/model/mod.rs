//! # The document model
//!
//! This module is derived from the `prosemirror-markdown` schema and the
//! the general JSON serialization of nodes.
pub mod steps;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeadingAttrs {
    pub level: u8,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodeBlockAttrs {
    pub params: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BulletListAttrs {
    tight: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrderedListAttrs {
    order: usize,
    tight: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImageAttrs {
    src: String,
    alt: String,
    title: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    Doc {
        #[serde(default = "Default::default")]
        content: Fragment,
    },
    Heading {
        attrs: HeadingAttrs,
        #[serde(default = "Default::default")]
        content: Fragment,
    },
    CodeBlock {
        attrs: CodeBlockAttrs,
        #[serde(default = "Default::default")]
        content: Fragment,
    },
    Text {
        text: String,
    },
    Blockquote {
        #[serde(default = "Default::default")]
        content: Fragment,
    },
    Paragraph {
        #[serde(default = "Default::default")]
        content: Fragment,
    },
    BulletList {
        #[serde(default = "Default::default")]
        content: Fragment,
        attrs: BulletListAttrs,
    },
    OrderedList {
        #[serde(default = "Default::default")]
        content: Fragment,
        attrs: OrderedListAttrs,
    },
    ListItem {
        #[serde(default = "Default::default")]
        content: Fragment,
    },
    HorizontalRule,
    HardBreak,
    Image {
        attrs: ImageAttrs,
    },
}

pub type Fragment = Vec<Node>;

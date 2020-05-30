//! # The document model
//!
//! This module is derived from the `prosemirror-markdown` schema and the
//! the general JSON serialization of nodes.
pub mod de;
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ImageAttrs {
    src: String,
    #[serde(default, deserialize_with = "de::deserialize_or_default")]
    alt: String,
    #[serde(default, deserialize_with = "de::deserialize_or_default")]
    title: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    Doc {
        #[serde(default)]
        content: Fragment,
    },
    Heading {
        attrs: HeadingAttrs,
        #[serde(default)]
        content: Fragment,
    },
    CodeBlock {
        attrs: CodeBlockAttrs,
        #[serde(default)]
        content: Fragment,
    },
    Text {
        text: String,
    },
    Blockquote {
        #[serde(default)]
        content: Fragment,
    },
    Paragraph {
        #[serde(default)]
        content: Fragment,
    },
    BulletList {
        #[serde(default)]
        content: Fragment,
        attrs: BulletListAttrs,
    },
    OrderedList {
        #[serde(default)]
        content: Fragment,
        attrs: OrderedListAttrs,
    },
    ListItem {
        #[serde(default)]
        content: Fragment,
    },
    HorizontalRule,
    HardBreak,
    Image {
        attrs: ImageAttrs,
    },
}

pub type Fragment = Vec<Node>;

#[cfg(test)]
mod tests {
    use super::ImageAttrs;

    #[test]
    fn test_null_string() {
        assert_eq!(
            serde_json::from_str::<ImageAttrs>(r#"{"src": "", "alt": null}"#).unwrap(),
            ImageAttrs {
                src: String::new(),
                title: String::new(),
                alt: String::new()
            }
        );
    }
}

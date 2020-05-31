use crate::model::{CodeBlockAttrs, HeadingAttrs, Node};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DocState {
    pub(super) doc: Node,
    pub(super) version: usize,
}

pub(super) fn initial_doc() -> Node {
    Node::Doc {
        content: vec![
            Node::Heading {
                attrs: HeadingAttrs {
                    level: 1,
                },
                content: vec![
                    Node::Text {
                        text: "Padington".to_string(),
                    }
                ]
            },
            Node::CodeBlock {
                attrs: CodeBlockAttrs {
                    params: String::new(),
                },
                content: vec![
                    Node::Text {
                        text: "fn foo(a: u32) -> u32 {\n  2 * a\n}".to_string(),
                    }
                ]
            },
            Node::Heading {
                attrs: HeadingAttrs {
                    level: 2,
                },
                content: vec![
                    Node::Text {
                        text: "Lorem Ipsum".to_string(),
                    }
                ]
            },
            Node::Blockquote {
                content: vec![
                    Node::Paragraph {
                        content: vec![
                            Node::Text {
                                text: String::from("Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet. Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet. Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet.")
                            }
                        ]
                    }
                ]
            }
        ]
    }
}

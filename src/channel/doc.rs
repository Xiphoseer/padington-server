use prosemirror::model::{CodeBlockAttrs, Fragment, HeadingAttrs, Node};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DocState {
    pub(super) doc: Node,
    pub(super) version: usize,
}

pub(super) fn initial_doc() -> Node {
    Node::Doc {
        content: Fragment::from(vec![
            Node::Heading {
                attrs: HeadingAttrs {
                    level: 1,
                },
                content: Fragment::from(vec![
                    Node::text("Padington"),
                ])
            },
            Node::CodeBlock {
                attrs: CodeBlockAttrs {
                    params: String::new(),
                },
                content: Fragment::from(vec![
                    Node::text("fn foo(a: u32) -> u32 {\n  2 * a\n}")
                ])
            },
            Node::Heading {
                attrs: HeadingAttrs {
                    level: 2,
                },
                content: Fragment::from(vec![
                    Node::text("Lorem Ipsum"),
                ])
            },
            Node::Blockquote {
                content: Fragment::from(vec![
                    Node::Paragraph {
                        content: Fragment::from(vec![
                            Node::text("Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet. Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet. Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet.")
                        ])
                    }
                ])
            }
        ])
    }
}

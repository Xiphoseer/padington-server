use prosemirror::markdown::{CodeBlockAttrs, HeadingAttrs, MarkdownNode};
use prosemirror::model::{Fragment, Node};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DocState {
    pub(super) doc: MarkdownNode,
    pub(super) version: usize,
}

pub(super) fn initial_doc() -> MarkdownNode {
    MarkdownNode::Doc {
        content: Fragment::from(vec![
            MarkdownNode::Heading {
                attrs: HeadingAttrs {
                    level: 1,
                },
                content: Fragment::from(vec![
                    Node::text("Padington"),
                ])
            },
            MarkdownNode::CodeBlock {
                attrs: CodeBlockAttrs {
                    params: String::new(),
                },
                content: Fragment::from(vec![
                    MarkdownNode::text("fn foo(a: u32) -> u32 {\n  2 * a\n}")
                ])
            },
            MarkdownNode::Heading {
                attrs: HeadingAttrs {
                    level: 2,
                },
                content: Fragment::from(vec![
                    MarkdownNode::text("Lorem Ipsum"),
                ])
            },
            MarkdownNode::Blockquote {
                content: Fragment::from(vec![
                    MarkdownNode::Paragraph {
                        content: Fragment::from(vec![
                            MarkdownNode::text("Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet. Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet. Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet.")
                        ])
                    }
                ])
            }
        ])
    }
}

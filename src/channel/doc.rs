use prosemirror::markdown::helper::{blockquote, code_block, doc, h1, h2, node, p, strong};
use prosemirror::markdown::MarkdownNode;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DocState {
    pub(super) doc: MarkdownNode,
    pub(super) version: usize,
}

pub(super) fn initial_doc() -> MarkdownNode {
    doc(vec![
        h1((
            "Padington",
        )),
        code_block("", (
            "fn foo(a: u32) -> u32 {\n  2 * a\n}",
        )),
        h2((
            "Lorem Ipsum",
        )),
        blockquote((
            p(vec![
                node("Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. "),
                strong("At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet."),
                node(" Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet. Lorem ipsum dolor sit amet, consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet."),
            ]),
        ))
    ])
}

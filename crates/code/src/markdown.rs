//! Embed the very same CodeViewer in Controls' Markdown extension registry. A host passes
//! its existing registry so Mermaid and other renderers remain installed.
use crate::{CodeDocument, CodeHandle, CodeOptions, CodeViewer, content_hash};
use gpui_kit::{
    ElementId, IntoElement, ParentElement, Styled,
    component::text::{MarkdownExtensions, MarkdownNode},
    div, px,
};
use guiraffe_core::StyleContext;
use std::sync::Arc;
const NODE: &str = "guiraffe-code";
#[derive(Clone)]
struct Fence {
    source: Arc<str>,
    language: Arc<str>,
    lines: usize,
}
struct FenceState(CodeHandle);
/// Install read-only code fences, retaining `mermaid` fences for the diagram extension.
/// Rendering, source selection/copy, shaping, highlighting and scrolling all reuse CodeViewer.
/// A block taller than `max_lines` scrolls within its own viewport.
pub fn code_fences(base: MarkdownExtensions, max_lines: usize) -> MarkdownExtensions {
    base.block_parser(|node, context| parse_fence(node, context.node_source(node)))
        .block_renderer(NODE, move |node, window, cx| {
            let Some(fence) = node.data::<Fence>() else {
                return div().into_any_element();
            };
            let hash = content_hash(fence.source.as_bytes());
            let offset = node.source_range().map(|r| r.start).unwrap_or_default();
            let state = window.use_keyed_state(
                ElementId::Name(format!("guiraffe-code/{offset}/{hash}").into()),
                cx,
                |_, cx| {
                    let code = CodeHandle::new(
                        hash,
                        Arc::new(CodeDocument::new(Arc::<str>::from(""))),
                        cx,
                    );
                    code.set_options(
                        CodeOptions {
                            margin_chars: 0,
                            minimap: false,
                            ..Default::default()
                        },
                        cx,
                    );
                    code.set_source(fence.source.clone(), fence.language.clone(), cx);
                    FenceState(code)
                },
            );
            let code = state.read(cx).0.clone();
            let size = StyleContext::current().control_size;
            div()
                .w_full()
                .h(px(fence.lines.clamp(1, max_lines.max(1)) as f32
                    * size.font_size()
                    * size.line_height()))
                .child(CodeViewer::new(&code))
                .into_any_element()
        })
}

fn parse_fence(node: &::markdown::mdast::Node, original: Option<&str>) -> Option<MarkdownNode> {
    let ::markdown::mdast::Node::Code(code) = node else {
        return None;
    };
    if code.lang.as_deref() == Some("mermaid") {
        return None;
    }
    let source: Arc<str> = code.value.as_str().into();
    let lines = source.bytes().filter(|b| *b == b'\n').count() + 1;
    let language: Arc<str> = code.lang.as_deref().unwrap_or("text").into();
    Some(
        MarkdownNode::new(
            NODE,
            Fence {
                source,
                language,
                lines,
            },
        )
        .text(code.value.clone())
        .markdown(original.map(str::to_owned).unwrap_or_else(|| {
            let longest = code
                .value
                .split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or_default();
            let fence = "`".repeat((longest + 1).max(3));
            format!(
                "{fence}{}\n{}\n{fence}",
                code.lang.as_deref().unwrap_or(""),
                code.value
            )
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn code_fences_preserve_source_and_leave_mermaid_for_its_renderer() {
        let tree = ::markdown::to_mdast(
            "```rust\nlet α = 1;\n```\n\n```mermaid\ngraph LR; a --> b\n```\n",
            &::markdown::ParseOptions::gfm(),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let ::markdown::mdast::Node::Root(root) = tree else {
            panic!("expected root")
        };
        let code = root
            .children
            .first()
            .and_then(|node| parse_fence(node, None))
            .unwrap_or_else(|| panic!("code routed to shared viewer"));
        assert_eq!(code.as_text(), "let α = 1;");
        assert_eq!(
            code.data::<Fence>().map(|f| f.language.as_ref()),
            Some("rust")
        );
        assert!(
            root.children
                .get(1)
                .and_then(|node| parse_fence(node, None))
                .is_none(),
            "Mermaid keeps its own component renderer"
        );
    }
}

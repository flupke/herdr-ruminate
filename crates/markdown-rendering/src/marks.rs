use markdown_marks::{Callout, Mark, StatusMark};
use ratatui_markdown::markdown::MarkdownBlock;

pub(super) struct ReadableMarks;

impl ReadableMarks {
    pub(super) fn apply(blocks: Vec<MarkdownBlock>) -> Vec<MarkdownBlock> {
        blocks.into_iter().map(Self::block).collect()
    }

    fn block(block: MarkdownBlock) -> MarkdownBlock {
        match block {
            MarkdownBlock::Blockquote {
                level,
                children,
                header_override,
                footer_override,
            } => MarkdownBlock::Blockquote {
                level,
                children: Self::quote(children),
                header_override,
                footer_override,
            },
            MarkdownBlock::Table { headers, rows } => MarkdownBlock::Table {
                headers: headers.into_iter().map(Self::cell).collect(),
                rows: rows
                    .into_iter()
                    .map(|row| row.into_iter().map(Self::cell).collect())
                    .collect(),
            },
            block => block,
        }
    }

    fn quote(children: Vec<MarkdownBlock>) -> Vec<MarkdownBlock> {
        let mut children = Self::apply(children);
        if let Some(MarkdownBlock::Paragraph(lines)) = children.first_mut()
            && let Some(first) = lines.first_mut()
            && let Some((callout, rest)) = Callout::strip(first)
        {
            *first = Self::joined(&format!("**{}:**", callout.label()), rest);
        }
        children
    }

    fn cell(cell: String) -> String {
        match StatusMark::strip(&cell) {
            Some((mark, rest)) => Self::joined(mark.symbol(), rest),
            None => cell,
        }
    }

    fn joined(shown: &str, rest: &str) -> String {
        if rest.is_empty() {
            shown.to_owned()
        } else {
            format!("{shown} {rest}")
        }
    }
}

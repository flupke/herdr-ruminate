use std::fmt::Write as _;

use markdown_marks::{Callout, Mark, StatusMark};
use review_explore::{ChangeAuthor, Comparison, TurnRequest};
use review_explore_round_settings::WritingStyle;
use review_repository::repository::SnapshotIdentity;

use crate::INDEX;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnFile {
    pub name: &'static str,
    pub text: String,
}

const TURN: &str = "turn.md";
const ROUND: &str = "round.md";

const READ_NOW: &str = "## Read now";
pub(crate) const READ_LATER: &str = "## Read when you need it";

enum When {
    Now(&'static str),
    On(&'static str),
}

struct Rule {
    name: &'static str,
    text: String,
    when: When,
}

impl Rule {
    fn new(name: &'static str, text: impl Into<String>, when: When) -> Self {
        Self {
            name,
            text: text.into(),
            when,
        }
    }
}

pub(super) fn files(
    request: &TurnRequest,
    comparison: &Comparison,
    input: &str,
    round: &str,
) -> Vec<TurnFile> {
    let rules = rules(request);
    let mut files = vec![
        TurnFile {
            name: INDEX,
            text: index(request, comparison, &rules),
        },
        TurnFile {
            name: TURN,
            text: format!("{}\n{input}", include_str!("rules/turn.md")),
        },
        TurnFile {
            name: ROUND,
            text: format!("{}\n{round}", include_str!("rules/round.md")),
        },
    ];
    files.extend(rules.into_iter().map(|rule| TurnFile {
        name: rule.name,
        text: rule.text,
    }));
    files
}

fn rules(request: &TurnRequest) -> Vec<Rule> {
    let drawing_board = request.at_drawing_board();
    let mut rules = vec![if request.is_kickoff() {
        Rule::new(
            "first-turn.md",
            include_str!("rules/first-turn.md"),
            When::Now("the steps of this turn"),
        )
    } else if drawing_board {
        Rule::new(
            "drawing-board.md",
            include_str!("rules/drawing-board.md"),
            When::Now("the steps of this turn, which ends the round"),
        )
    } else {
        Rule::new(
            "next-turn.md",
            include_str!("rules/next-turn.md"),
            When::Now("the steps of this turn, and how to take up the reviewer's answer"),
        )
    }];
    if request.author == ChangeAuthor::Other {
        rules.push(Rule::new(
            "others-change.md",
            include_str!("rules/others-change.md"),
            When::Now("how a review of someone else's change differs, which overrides the rest"),
        ));
    }
    if request.challenger && !drawing_board {
        rules.push(Rule::new(
            "challenger.md",
            include_str!("rules/challenger.md"),
            When::Now("how you and the challenger share each turn"),
        ));
    }
    rules.push(Rule::new(
        "diagrams.md",
        diagrams(),
        When::On("before you draw a diagram"),
    ));
    if !drawing_board {
        rules.push(Rule::new(
            "conclusion.md",
            include_str!("rules/conclusion.md"),
            When::On("before you conclude"),
        ));
    }
    rules.push(Rule::new(
        "conversation.md",
        include_str!("rules/conversation.md"),
        When::On("when get_new_messages returns this round's conversation"),
    ));
    rules
}

fn index(request: &TurnRequest, comparison: &Comparison, rules: &[Rule]) -> String {
    let kickoff = request.is_kickoff();
    let data = if kickoff {
        "this turn's identity and unreviewed diffs"
    } else {
        "this turn's identity and unreviewed diffs, and the reviewer's answer"
    };
    let round = "the change, its author, the reset rounds, the earlier decisions and the reviewer \
                 notes";
    let mut index = format!(
        "{}\n{READ_NOW}\n\n- `{TURN}`: {data}.\n",
        include_str!("rules/index.md")
    );
    if kickoff {
        let _ = writeln!(index, "- `{ROUND}`: {round}.");
    }
    for rule in rules {
        if let When::Now(about) = rule.when {
            let _ = writeln!(index, "- `{}`: {about}.", rule.name);
        }
    }
    let _ = write!(index, "\n{READ_LATER}\n\n");
    if !kickoff {
        let _ = writeln!(
            index,
            "- `{ROUND}`: {round}, when they are no longer in your context."
        );
    }
    for rule in rules {
        if let When::On(condition) = rule.when {
            let _ = writeln!(index, "- `{}`: {condition}.", rule.name);
        }
    }
    let _ = write!(index, "\n{}", include_str!("rules/delivery.md"));
    if kickoff {
        let _ = write!(index, "\n{}", include_str!("rules/design.md"));
    }
    let _ = write!(
        index,
        "\n{}\n{}\n{}\n{}",
        include_str!("rules/questions.md"),
        sources(comparison),
        texts(request.writing),
        include_str!("rules/not-relevant.md"),
    );
    index
}

fn sources(comparison: &Comparison) -> String {
    let jj = include_str!("rules/sources-jj.md");
    let git = include_str!("rules/sources-git.md");
    let commands = match comparison.base {
        Some(SnapshotIdentity::Jj { .. }) => jj.to_owned(),
        Some(SnapshotIdentity::Git { .. }) => git.to_owned(),
        None => format!("{jj}\n{git}"),
    };
    format!("{}\n{commands}", include_str!("rules/sources.md"))
}

fn texts(writing: WritingStyle) -> String {
    fn markers<M: Mark>() -> String {
        M::ALL
            .iter()
            .map(|mark| format!("`{}`", mark.marker()))
            .collect::<Vec<_>>()
            .join(", ")
    }
    let mut texts = format!(
        "{}\nA callout is a quote that opens with its marker, as in `> {} Run it twice.`: {}.\n\
         A status mark opens a table cell, as in `| {} 2 ms |`: {}.\n",
        include_str!("rules/texts.md"),
        Callout::Tip.marker(),
        markers::<Callout>(),
        StatusMark::Good.marker(),
        markers::<StatusMark>(),
    );
    let style = match writing {
        WritingStyle::Plain => None,
        WritingStyle::SimplifiedTechnicalEnglish => Some(include_str!(
            "rules/writing-style-simplified-technical-english.md"
        )),
        WritingStyle::Concise => Some(include_str!("rules/writing-style-concise.md")),
    };
    if let Some(style) = style {
        let _ = write!(texts, "\n{style}");
    }
    texts
}

fn diagrams() -> String {
    format!(
        "# Diagrams\n\nA diagram is a fenced block that opens with ```` ```{} ````, which the \
         Explore page draws with Mermaid {}.\n{}",
        mermaid_js::FENCE,
        mermaid_js::VERSION,
        include_str!("rules/diagrams.md"),
    )
}

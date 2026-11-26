use std::path::Path;

use review_explore::{ChangeAuthor, Comparison, TurnRequest};
use review_explore_model::ExploreModels;

mod decisions;
mod input;
mod rules;
mod unreviewed;

pub use decisions::EarlierDecisions;
pub use rules::TurnFile;
pub use unreviewed::Unreviewed;

#[derive(Debug, Default)]
pub struct RoundData {
    pub earlier: EarlierDecisions,
    pub notes: Option<String>,
}

pub const INDEX: &str = "index.md";

#[derive(Clone, Debug)]
pub struct TurnInstructions {
    instance: String,
    files: Vec<TurnFile>,
    models: Option<ExploreModels>,
}

impl TurnInstructions {
    pub fn prepare(
        request: &TurnRequest,
        comparison: &Comparison,
        access: &str,
        unreviewed: &Unreviewed,
        round: &RoundData,
        answered: Option<review_types::QuestionNumber>,
    ) -> Self {
        let round = input::Round {
            repository_root: &comparison.repository_root,
            description: comparison.change_description(),
            decisions: &round.earlier,
            author: request.author,
            reviewer: request.reviewer.as_deref(),
            notes: round.notes.as_deref(),
        };
        let input = input::TurnInput {
            request,
            access,
            unreviewed,
            answered,
        };
        Self {
            instance: request.instance.clone(),
            files: rules::files(request, comparison, &input.to_string(), &round.to_string()),
            models: None,
        }
    }

    #[must_use]
    pub fn on_models(self, models: Option<ExploreModels>) -> Self {
        Self { models, ..self }
    }

    pub fn files(&self) -> &[TurnFile] {
        &self.files
    }

    // Claude Code and Codex show MCP tools only under names that hold their server's name.
    pub fn prompt(&self, directory: &Path) -> String {
        let model = self
            .models
            .as_ref()
            .map(|models| format!("{}\n", models.round_line()))
            .unwrap_or_default();
        format!(
            "Explore round {}: read {} and follow it. The turn ends in one call to submit_question \
             or submit_conclusion, tools of the {} MCP server.\n{model}",
            self.instance,
            directory.join(INDEX).display(),
            review_types::MCP_SERVER,
        )
    }
}

pub fn implementation_prompt(
    request: &review_explore::ImplementationRequest,
    author: ChangeAuthor,
    models: Option<&ExploreModels>,
) -> String {
    let round = models
        .map(|models| format!("{}\n\n", models.round_line()))
        .unwrap_or_default();
    if author == ChangeAuthor::Other {
        return format!(
            "The reviewer clicked Post review on the Explore conclusion of someone else's change. Post the review below on the change's pull request, with the repository's usual tool for it, such as gh: each comment on the file and line it names, the others in the review's body. This action authorizes posting these comments only: do not edit code, push, approve or request changes. If you cannot find the change's pull request, say so and stop. Close with a short report that says what you posted and where.\n\n{round}Review from the reviewer:\n{}",
            request.text
        );
    }
    let delegation = models
        .and_then(|models| models.worker.as_ref())
        .map(|worker| {
            format!(
                "Have one fresh subagent do the implementation: start it with the Agent tool, with `model` `{}` and `effort` `{}`, the worker's. It starts with no context, so its prompt is a whole brief: the task list below as the reviewer edited it, word for word; the decisions of this round that bear on it, with their reasons; the repository's instructions it must follow (its AGENTS.md or CLAUDE.md, and the files they name); and how to validate the changes. Once it reports, review its work yourself before your report: read its diff, check that the validation it ran passed, and send it what to fix until the changes do what the tasks ask.\n\n",
                worker.model, worker.effort
            )
        })
        .unwrap_or_default();
    format!(
        "The reviewer clicked Implement on the Explore conclusion. Implement the edited task list below in this conversation, following the repository instructions and validating the changes. This action authorizes code edits and tests for these tasks. Summary and future-work items are not additional implementation scope. Do not submit another review question or mark files reviewed as part of this action. Close with a short report for a reader who did not watch you work and has not seen your tool calls: first what changed for each task, then how you checked it and what is left, in plain full sentences, with no term you made up during the work.\n\n{delegation}{round}Tasks from the reviewer:\n{}",
        request.text
    )
}

#[cfg(test)]
mod tests;

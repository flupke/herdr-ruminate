//! What the reviewer, the person, writes to the agent in an Explore round goes on herdr-bus
//! too, as the review comments do, so that other programs, such as herdr-bowtie, read it.

use std::collections::HashSet;

use herdr_client::protocol::PaneId;
use review_explore::{ExploreRound, ImplementationRequest, TurnRequest};
use review_thread_service::PinnedAgent;
use review_words_bus::{Place, UserWords};

use crate::ExploreSession;

impl ExploreSession {
    /// Publishes what the reviewer wrote in the new turn `request`, which the round saved, to the
    /// agent the round selected for it. Of an answer, its text: a choice's comment, a comment
    /// alone, or a reply to the conclusion; of Back to the drawing board, the comment typed with
    /// it, the reviewer's reason, and a reply to the conclusion it reached. Of a kickoff, the
    /// reviewer's name, for someone else's change. A turn without the reviewer's words publishes
    /// nothing, and a cancelled answer does not withdraw what it published: the bus is a log.
    pub(crate) fn publish_turn(&self, request: &TurnRequest) {
        let to = self.agent_pane();
        let Some(answer) = &request.answer else {
            if let Some(name) = &request.reviewer {
                self.prompts.publish_words(UserWords {
                    id: request.request.clone(),
                    text: name.clone(),
                    review_unit: request.checkpoint.review_unit.clone(),
                    place: Place::Start {
                        round: request.instance.clone(),
                    },
                    to,
                });
            }
            return;
        };
        if answer.text.trim().is_empty() {
            return;
        }
        self.prompts.publish_words(UserWords {
            id: answer.id.clone(),
            text: answer.text.clone(),
            review_unit: request.checkpoint.review_unit.clone(),
            place: Place::Answer {
                round: request.instance.clone(),
                question: answer.question.as_ref().map(|question| question.id.clone()),
                choice: answer.option.as_ref().map(|choice| choice.id.clone()),
                drawing_board: answer.drawing_board,
            },
            to,
        });
    }

    /// Publishes the lines of the list of `request`, saved in `round`, that the reviewer wrote or
    /// changed, to the agent the round selected for it: the agent's own lines, and their
    /// @mentions, stay off the bus, and a list sent as the agent wrote it publishes nothing.
    pub(crate) fn publish_implementation(
        &self,
        request: &ImplementationRequest,
        round: &ExploreRound,
    ) {
        let agents_lines: HashSet<&str> = round
            .exploration
            .conclusion
            .as_ref()
            .map(|conclusion| {
                conclusion
                    .to_be_implemented
                    .lines()
                    .map(str::trim)
                    .collect()
            })
            .unwrap_or_default();
        let written: Vec<&str> = request
            .text
            .lines()
            .filter(|line| !line.trim().is_empty() && !agents_lines.contains(line.trim()))
            .collect();
        if written.is_empty() {
            return;
        }
        self.prompts.publish_words(UserWords {
            id: request.delivery.clone(),
            text: written.join("\n"),
            review_unit: round.exploration.comparison.checkpoint.review_unit.clone(),
            place: Place::Implementation {
                round: request.instance.clone(),
                conclusion: request.conclusion.clone(),
            },
            to: self.agent_pane(),
        });
    }

    /// The pane of the agent the round selected last, if any.
    fn agent_pane(&self) -> Option<PaneId> {
        self.state
            .agent
            .as_ref()
            .and_then(PinnedAgent::known_agent)
            .map(|agent| agent.pane_id)
    }
}

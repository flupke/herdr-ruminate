use review_explore::Command;
use review_store::LoadResult;
use review_types::MarkAuthor;

use super::*;

fn retaking_conclusion(request: &TurnRequest) -> Operation {
    let Operation::SubmitConclusion(mut submission) = marking_conclusion(request) else {
        unreachable!("a conclusion");
    };
    submission.reopened = submission.reviewed.clone();
    Operation::SubmitConclusion(submission)
}

impl Harness {
    fn answered(&mut self) -> (TurnRequest, String) {
        self.capture();
        let first = self.request(None);
        let access = self.turn(&first);
        assert!(applied(self.submit(&access, question(&first, 1))));
        self.answer("Keep it.")
    }

    fn cancel(&mut self, answer: &str) -> Result<Arc<ExploreRound>, String> {
        self.session
            .handle(Input::Command(Command::CancelAnswer(answer.to_owned())));
        let cancelled = self.next::<ui_events::ExploreAnswerCancelled>();
        assert_eq!(cancelled.answer, answer);
        if let Ok(round) = &cancelled.result {
            self.exploration = Some(round.exploration.clone());
        }
        cancelled.result
    }

    fn author(&self) -> Option<MarkAuthor> {
        match self.store.load(&self.unit, b"reviewed.rs").unwrap() {
            LoadResult::Reviewed(record) => Some(record.author),
            _ => None,
        }
    }

    fn mark_as_reviewer(&self) {
        let snapshot = complete_repository_snapshot(&self.repository);
        let tracker = review_state::ReviewTracker::new(self.repository.clone(), self.store.clone());
        tracker
            .mark(&snapshot, &snapshot.files[0], &MarkAuthor::Reviewer)
            .unwrap();
    }
}

#[test]
fn cancelling_an_answer_reopens_its_lines_and_the_next_prompt_says_so() {
    let mut harness = Harness::start();
    let (request, access) = harness.answered();
    assert!(applied(
        harness.submit(&access, marking_conclusion(&request))
    ));
    let answer = request.answer.unwrap().id;
    assert!(harness.author().is_some());

    let round = harness.cancel(&answer).unwrap();

    assert_eq!(harness.author(), None);
    assert!(round.exploration.answers.is_empty() && round.exploration.conclusion.is_none());
    assert!(round.marks.is_empty() && round.completion.is_none());
    assert_eq!(harness.saved(), *round);
    let (replacement, _) = harness.answer("Keep it, with a test.");
    assert_eq!(replacement.cancelled, vec![answer.clone()]);
    let prompt = harness.agents.prompts().last().unwrap().text.clone();
    assert!(
        prompt.contains(&format!("Cancelled answer: {answer}\n")),
        "{prompt}"
    );
}

#[test]
fn lines_the_answer_took_over_return_to_their_previous_author() {
    let mut harness = Harness::start();
    harness
        .files
        .write("unreviewed.rs", b"pub fn unreviewed() {}\n");
    harness.mark_as_reviewer();
    let (request, access) = harness.answered();
    assert!(applied(
        harness.submit(&access, retaking_conclusion(&request))
    ));
    let answer = request.answer.unwrap().id;
    assert_eq!(
        harness.author(),
        Some(MarkAuthor::Explore {
            answer: answer.clone()
        })
    );

    harness.cancel(&answer).unwrap();

    assert_eq!(harness.author(), Some(MarkAuthor::Reviewer));
}

#[test]
fn lines_the_answer_took_over_return_once_another_file_changed() {
    let mut harness = Harness::start();
    harness
        .files
        .write("unreviewed.rs", b"pub fn unreviewed() {}\n");
    harness.mark_as_reviewer();
    let (request, access) = harness.answered();
    assert!(applied(
        harness.submit(&access, retaking_conclusion(&request))
    ));
    let answer = request.answer.unwrap().id;
    harness
        .files
        .write("unreviewed.rs", b"// note\npub fn unreviewed() {}\n");

    harness.cancel(&answer).unwrap();

    assert_eq!(harness.author(), Some(MarkAuthor::Reviewer));
}

#[test]
fn an_answer_cancelled_mid_turn_rejects_the_agents_late_reply() {
    let mut harness = Harness::start();
    let (request, access) = harness.answered();
    let answer = request.answer.as_ref().unwrap().id.clone();

    harness.cancel(&answer).unwrap();

    assert!(
        harness
            .submit(&access, conclusion(&request, CONCLUSION))
            .is_err()
    );
    let saved = harness.saved();
    assert!(saved.exploration.answers.is_empty() && saved.turns.len() == 1);
}

#[test]
fn the_pane_cancels_only_the_latest_answer() {
    let mut harness = Harness::start();
    let (request, access) = harness.answered();
    assert!(applied(harness.submit(&access, question(&request, 2))));
    let (_, _) = harness.answer("Second answer.");
    let first = request.answer.unwrap().id;

    let error = harness.cancel(&first).unwrap_err();

    assert!(error.contains("Only the latest answer"), "{error}");
    assert_eq!(harness.saved().exploration.answers.len(), 2);
}

#[test]
fn cancelling_an_answer_also_reopens_what_its_turn_found_not_relevant() {
    let mut harness = Harness::start();
    let (request, access) = harness.answered();
    let Operation::SubmitConclusion(mut submission) = conclusion(&request, CONCLUSION) else {
        unreachable!("a conclusion");
    };
    submission.not_relevant = vec![
        serde_json::from_value(serde_json::json!({
            "path": "reviewed.rs", "side": "new", "lines": null, "reason": "follows_code"
        }))
        .unwrap(),
    ];
    assert!(applied(
        harness.submit(&access, Operation::SubmitConclusion(submission))
    ));
    assert!(harness.author().is_some());

    harness.cancel(&request.answer.unwrap().id).unwrap();

    assert_eq!(harness.author(), None);
}

#[test]
fn lines_found_not_relevant_after_an_answer_carry_that_answers_author() {
    let mut harness = Harness::start();
    let (request, access) = harness.answered();
    let Operation::SubmitConclusion(mut submission) = conclusion(&request, CONCLUSION) else {
        unreachable!("a conclusion");
    };
    submission.not_relevant = vec![
        serde_json::from_value(serde_json::json!({
            "path": "reviewed.rs", "side": "new", "lines": {"first_line": 1, "last_line": 1}, "reason": "follows_code"
        }))
        .unwrap(),
    ];

    assert!(applied(
        harness.submit(&access, Operation::SubmitConclusion(submission))
    ));

    let answer = request.answer.unwrap().id;
    let marks = &harness.saved().marks[&request.request];
    assert_eq!(marks.counts().not_relevant_lines, 1);
    assert!(marks.reviewed.is_empty());
    assert_eq!(harness.author(), Some(MarkAuthor::Explore { answer }));
}

#[test]
fn cancelling_an_answer_gives_back_the_marks_of_the_question_it_answered() {
    let mut harness = Harness::start();
    harness.capture();
    let first = harness.request(None);
    let access = harness.turn(&first);
    let Operation::SubmitQuestion(mut update) = question(&first, 1) else {
        unreachable!("a question");
    };
    update.not_relevant = vec![
        serde_json::from_value(serde_json::json!({
            "path": "reviewed.rs", "side": "new", "lines": null, "reason": "follows_code"
        }))
        .unwrap(),
    ];
    assert!(applied(
        harness.submit(&access, Operation::SubmitQuestion(update))
    ));
    let (request, _) = harness.answer("Keep it.");
    let answer = request.answer.unwrap().id;
    assert_eq!(
        harness.author(),
        Some(MarkAuthor::Explore {
            answer: answer.clone()
        })
    );

    let round = harness.cancel(&answer).unwrap();

    assert_eq!(harness.author(), None);
    assert!(round.marks.is_empty());

    let (replacement, _) = harness.answer("Keep it, with a test.");
    let answer = replacement.answer.unwrap().id;
    assert_eq!(harness.author(), Some(MarkAuthor::Explore { answer }));
}

#[test]
fn after_a_cancelled_answer_the_agent_revises_the_question_the_round_went_back_to() {
    let mut harness = Harness::start();
    let (request, access) = harness.answered();
    assert!(applied(harness.submit(&access, question(&request, 2))));

    harness
        .cancel(&request.answer.as_ref().unwrap().id)
        .unwrap();

    let revised = harness.submit(&access, revised_first_question(&request));
    assert_eq!(shown_as(revised).as_deref(), Some("Q1"));
}

#[test]
fn an_answer_cancelled_mid_turn_lets_the_agent_revise_the_question_the_round_went_back_to() {
    let mut harness = Harness::start();
    let (request, access) = harness.answered();

    harness
        .cancel(&request.answer.as_ref().unwrap().id)
        .unwrap();

    let revised = harness.submit(&access, revised_first_question(&request));
    assert_eq!(shown_as(revised).as_deref(), Some("Q1"));
}

#[test]
fn after_stop_waiting_and_a_cancelled_answer_the_agent_revises_the_question_the_round_went_back_to()
{
    let mut harness = Harness::start();
    let (request, access) = harness.answered();
    harness.session.handle(Input::Command(Command::Cancel));

    harness
        .cancel(&request.answer.as_ref().unwrap().id)
        .unwrap();

    let revised = harness.submit(&access, revised_first_question(&request));
    assert_eq!(shown_as(revised).as_deref(), Some("Q1"));
}

#[test]
fn after_cancelling_an_answer_whose_prompt_failed_the_agent_revises_with_its_earlier_access() {
    let mut harness = Harness::start();
    harness.capture();
    let first = harness.request(None);
    let access = harness.turn(&first);
    assert!(applied(harness.submit(&access, question(&first, 1))));
    harness.agents.refuse_prompts("it waits on its user");
    let answer = harness.request(Some(AnswerInput {
        option: Some("keep".into()),
        ..AnswerInput::default()
    }));
    let posted = harness.post_turn(&answer);
    harness.threads.prompt_sender().flush();
    assert_eq!(
        harness.agents.prompts().len(),
        1,
        "only the kickoff reached the agent"
    );

    harness.cancel(&posted.answer.unwrap().id).unwrap();

    let revised = harness.submit(&access, revised_first_question(&first));
    assert_eq!(shown_as(revised).as_deref(), Some("Q1"));
}

#[test]
fn after_cancelling_an_answer_whose_prompt_was_withdrawn_the_agent_revises_with_its_earlier_access()
{
    let mut harness = Harness::start();
    harness.capture();
    let first = harness.request(None);
    let access = harness.turn(&first);
    assert!(applied(harness.submit(&access, question(&first, 1))));
    let hold = harness.threads.prompt_sender().hold();
    let answer = harness.request(Some(AnswerInput {
        option: Some("keep".into()),
        ..AnswerInput::default()
    }));
    let posted = harness.post_turn(&answer);
    harness.threads.prompt_sender().flush();

    harness.cancel(&posted.answer.unwrap().id).unwrap();
    drop(hold);
    harness.threads.prompt_sender().flush();

    assert_eq!(
        harness.agents.prompts().len(),
        1,
        "the answer's prompt was withdrawn"
    );
    let revised = harness.submit(&access, revised_first_question(&first));
    assert_eq!(shown_as(revised).as_deref(), Some("Q1"));
}

#[test]
fn the_next_answer_after_a_cancelled_one_takes_back_the_access_the_agent_held() {
    let mut harness = Harness::start();
    let (request, access) = harness.answered();
    assert!(applied(harness.submit(&access, question(&request, 2))));
    harness
        .cancel(&request.answer.as_ref().unwrap().id)
        .unwrap();
    let hold = harness.threads.prompt_sender().hold();
    let again = harness.request(Some(AnswerInput {
        option: Some("keep".into()),
        text: "Keep it after all.".into(),
        ..AnswerInput::default()
    }));
    let posted = harness.post_turn(&again);

    let stale = harness.submit(&access, question(&posted, 2)).unwrap_err();

    assert!(stale.contains("Obsolete Explore access"), "{stale}");
    drop(hold);
}

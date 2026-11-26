use super::*;

impl Harness {
    fn publish_on_bus(&self, bus: &std::path::Path) {
        self.threads.publish_on_bus(Ok(bus.to_owned()));
    }

    fn words_on_bus(&self, bus: &std::path::Path) -> Vec<herdr_bus::Message> {
        self.threads.prompt_sender().flush();
        let mut stored = herdr_bus::Bus::open(bus)
            .unwrap()
            .messages(&herdr_bus::MessageQuery {
                output: Some(review_words_bus::OUTPUT.into()),
                ..herdr_bus::MessageQuery::default()
            })
            .unwrap();
        stored.reverse();
        stored.into_iter().map(|stored| stored.message).collect()
    }

    fn answer_with(&mut self, answer: AnswerInput) -> (TurnRequest, String) {
        let request = self.request(Some(answer));
        let access = self.turn(&request);
        let request = self.exploration().retry_request().unwrap().clone();
        (request, access)
    }

    fn reply_to_conclusion(&mut self, text: &str) -> TurnRequest {
        let reply = self
            .exploration
            .as_mut()
            .unwrap()
            .request(
                Some(AnswerInput {
                    text: text.into(),
                    ..AnswerInput::default()
                }),
                None,
            )
            .unwrap();
        self.turn(&reply);
        self.exploration().retry_request().unwrap().clone()
    }
}

fn words(message: &herdr_bus::Message) -> (String, &str, Option<&str>, Option<&str>) {
    (
        message.id.to_string(),
        message.data["text"].as_str().unwrap(),
        message.data["to"].as_str(),
        message.data["place"].as_str(),
    )
}

#[test]
fn an_answer_s_comment_a_comment_alone_and_the_lines_added_to_the_list_go_on_the_bus() {
    let directory = tempfile::tempdir().unwrap();
    let bus = directory.path().join("bus.db");
    let mut harness = Harness::start();
    harness.publish_on_bus(&bus);
    harness.capture();
    let first = harness.request(None);
    let access = harness.turn(&first);
    assert!(applied(harness.submit(&access, question(&first, 1))));
    let (with_choice, access) = harness.answer("Keep it, @David checks the timeout.");
    assert!(applied(harness.submit(&access, question(&with_choice, 2))));
    let (alone, access) = harness.answer_with(AnswerInput {
        text: "Neither: ask @Cindy.".into(),
        ..AnswerInput::default()
    });
    assert!(applied(
        harness.submit(&access, conclusion(&alone, CONCLUSION))
    ));
    let conclusion = harness.conclusion_request();
    assert_eq!(
        harness.implement_on_page(
            &conclusion,
            None,
            "  Add a regression test.\nAnd tell @David.\nAnd ask @Cindy.\n"
        ),
        Ok(())
    );
    harness.implementation_finished();

    let on_bus = harness.words_on_bus(&bus);
    let pane = format!("pane:{PANE}");
    assert_eq!(
        on_bus.iter().map(words).collect::<Vec<_>>(),
        [
            (
                with_choice.answer.as_ref().unwrap().id.clone(),
                "Keep it, @David checks the timeout.",
                Some(pane.as_str()),
                Some("answer"),
            ),
            (
                alone.answer.as_ref().unwrap().id.clone(),
                "Neither: ask @Cindy.",
                Some(pane.as_str()),
                Some("answer"),
            ),
            (
                harness
                    .saved()
                    .latest_implementation(&conclusion)
                    .unwrap()
                    .request
                    .delivery
                    .clone(),
                "And tell @David.\nAnd ask @Cindy.",
                Some(pane.as_str()),
                Some("implementation"),
            ),
        ]
    );
    let round = harness.exploration().instance.clone();
    assert_eq!(on_bus[0].data["round"], round.as_str());
    assert_eq!(on_bus[0].data["question"], "q1");
    assert_eq!(on_bus[0].data["choice"], "keep");
    assert_eq!(on_bus[1].data["question"], "q2");
    assert_eq!(on_bus[1].data.get("choice"), None);
    assert_eq!(on_bus[2].data["conclusion"], conclusion.as_str());
}

#[test]
fn a_choice_without_comment_and_the_agent_s_own_list_publish_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let bus = directory.path().join("bus.db");
    let mut harness = Harness::start();
    harness.publish_on_bus(&bus);
    harness.capture();
    let first = harness.request(None);
    let access = harness.turn(&first);
    assert!(applied(harness.submit(&access, question(&first, 1))));
    let (answered, access) = harness.answer("");
    assert!(applied(
        harness.submit(&access, conclusion(&answered, CONCLUSION))
    ));
    let conclusion = harness.conclusion_request();
    let agents_list = harness
        .saved()
        .exploration
        .conclusion
        .unwrap()
        .to_be_implemented;

    assert_eq!(
        harness.implement_on_page(&conclusion, None, &format!("{agents_list}\n")),
        Ok(())
    );
    harness.implementation_finished();

    assert_eq!(harness.words_on_bus(&bus), Vec::new());
}

#[test]
fn back_at_the_drawing_board_a_reply_to_the_conclusion_goes_on_the_bus_marked_so() {
    let directory = tempfile::tempdir().unwrap();
    let bus = directory.path().join("bus.db");
    let mut harness = Harness::start();
    harness.publish_on_bus(&bus);
    harness.capture();
    let first = harness.request(None);
    let access = harness.turn(&first);
    assert!(applied(harness.submit(&access, question(&first, 1))));
    let (back, access) = harness.answer_with(AnswerInput {
        drawing_board: true,
        ..AnswerInput::default()
    });
    assert!(applied(
        harness.submit(&access, drawing_board_conclusion(&back))
    ));

    let reply = harness.reply_to_conclusion("And ask @David about the queue.");

    let on_bus = harness.words_on_bus(&bus);
    assert_eq!(on_bus.len(), 1, "sent back with no comment, it has no text");
    assert_eq!(on_bus[0].id.to_string(), reply.answer.unwrap().id);
    assert_eq!(on_bus[0].data["text"], "And ask @David about the queue.");
    assert_eq!(on_bus[0].data["drawing_board"], true);
    assert_eq!(on_bus[0].data.get("question"), None);
}

#[test]
fn the_reason_typed_for_back_to_the_drawing_board_goes_on_the_bus_with_its_question() {
    let directory = tempfile::tempdir().unwrap();
    let bus = directory.path().join("bus.db");
    let mut harness = Harness::start();
    harness.publish_on_bus(&bus);
    harness.capture();
    let first = harness.request(None);
    let access = harness.turn(&first);
    assert!(applied(harness.submit(&access, question(&first, 1))));

    let (back, _) = harness.answer_with(AnswerInput {
        text: "One queue per conversation.".into(),
        drawing_board: true,
        ..AnswerInput::default()
    });

    let on_bus = harness.words_on_bus(&bus);
    assert_eq!(on_bus.len(), 1);
    assert_eq!(on_bus[0].id.to_string(), back.answer.unwrap().id);
    assert_eq!(on_bus[0].data["text"], "One queue per conversation.");
    assert_eq!(on_bus[0].data["drawing_board"], true);
    assert_eq!(on_bus[0].data["question"], "q1");
}

#[test]
fn the_name_given_at_the_start_goes_on_the_bus_with_the_kickoff() {
    let directory = tempfile::tempdir().unwrap();
    let bus = directory.path().join("bus.db");
    let mut harness = Harness::start();
    harness.publish_on_bus(&bus);
    let choices =
        review_explore::RoundChoices::new(false, review_explore::ChangeAuthor::Other, "Viktor");

    assert_eq!(harness.start_on_page_with(choices), Ok(()));
    let round = harness
        .next::<ui_events::ExplorePosted>()
        .result
        .expect("the kickoff is saved");
    let kickoff = round.exploration.retry_request().unwrap().clone();
    harness.exploration = Some(round.exploration.clone());
    harness.delivered_prompt();

    let on_bus = harness.words_on_bus(&bus);
    assert_eq!(
        on_bus.iter().map(words).collect::<Vec<_>>(),
        [(
            kickoff.request.clone(),
            "Viktor",
            Some(format!("pane:{PANE}").as_str()),
            Some("start"),
        )]
    );
}

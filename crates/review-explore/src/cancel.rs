use crate::{Exploration, ExploreRound, ReviewerAnswer, TurnMarks};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelledAnswer {
    pub answer: ReviewerAnswer,
    pub request: Option<String>,
    pub marks: Vec<TurnMarks>,
}

impl Exploration {
    fn cancel_answer(&mut self, id: &str) -> eyre::Result<CancelledAnswer> {
        eyre::ensure!(
            self.answers.last().is_some_and(|answer| answer.id == id),
            "Only the latest answer can be cancelled"
        );
        let answer = self.answers.pop().expect("latest answer");
        let carries = |request: &&crate::TurnRequest| {
            request
                .answer
                .as_ref()
                .is_some_and(|carried| carried.id == id)
        };
        let pending = self
            .outstanding
            .as_ref()
            .filter(carries)
            .or(self.retry.as_ref().filter(carries))
            .cloned();
        if pending.is_some() {
            self.outstanding = None;
            self.retry = None;
        }
        let answered = self
            .conversation
            .iter()
            .find(|turn| turn.answer.as_deref() == Some(id))
            .map(|turn| turn.update.request.clone());
        let request = answered
            .clone()
            .or_else(|| pending.as_ref().map(|request| request.request.clone()));
        if answered.is_some() {
            self.conversation
                .retain(|turn| turn.answer.as_deref() != Some(id));
            self.replay();
        }
        if let Some(pending) = pending {
            self.cancelled.splice(0..0, pending.cancelled);
        }
        self.cancelled.push(answer.id.clone());
        Ok(CancelledAnswer {
            answer,
            request,
            marks: Vec::new(),
        })
    }

    fn replay(&mut self) {
        self.topics.clear();
        self.interpretations.clear();
        self.questions.clear();
        self.limitations.clear();
        self.findings.clear();
        self.glossary.clear();
        self.conclusion = None;
        for turn in self.conversation.clone() {
            let answer = turn
                .answer
                .as_ref()
                .and_then(|id| self.answers.iter().find(|answer| &answer.id == id))
                .cloned();
            self.absorb(turn.update, answer.as_ref());
        }
        self.forget_diagram_errors_of_withdrawn_questions();
        self.forget_quiz_answers_of_withdrawn_conclusions();
    }
}

impl ExploreRound {
    pub fn cancel_answer(&mut self, id: &str) -> eyre::Result<CancelledAnswer> {
        eyre::ensure!(
            self.implementations.is_empty(),
            "Implementation was requested; answers can no longer be cancelled"
        );
        let mut cancelled = self.exploration.cancel_answer(id)?;
        let mut requests: Vec<String> = self
            .turns
            .iter()
            .filter(|(_, turn)| turn.request.answer.as_ref().is_some_and(|a| a.id == id))
            .map(|(request, _)| request.clone())
            .collect();
        requests.extend(cancelled.request.clone());
        for request in &requests {
            self.turns.remove(request);
            cancelled.marks.extend(self.marks.remove(request));
            self.exploration.agent_elapsed_ms.remove(request);
            if self
                .completion
                .as_ref()
                .is_some_and(|completion| &completion.request == request)
            {
                self.completion = None;
            }
        }
        let question = &cancelled.answer.in_reply_to;
        if self
            .marks
            .get(question)
            .is_some_and(|marks| marks.answer.as_deref() == Some(id))
        {
            cancelled.marks.extend(self.marks.remove(question));
        }
        Ok(cancelled)
    }

    pub fn cancel_answers_from(&mut self, id: &str) -> eyre::Result<Vec<CancelledAnswer>> {
        eyre::ensure!(
            self.exploration
                .answers
                .iter()
                .any(|answer| answer.id == id),
            "This answer is not in the round"
        );
        let mut cancelled = Vec::new();
        while let Some(latest) = self
            .exploration
            .answers
            .last()
            .map(|answer| answer.id.clone())
        {
            cancelled.push(self.cancel_answer(&latest)?);
            if latest == id {
                break;
            }
        }
        Ok(cancelled)
    }
}

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, PoisonError};

use review_explore::{Alternative, Door, Question};

pub(crate) struct BlindQuestion<'a>(&'a Question);

impl<'a> BlindQuestion<'a> {
    pub(crate) fn of(question: &'a Question) -> Option<Self> {
        let door = question.assessments.as_ref()?.door;
        let recommends = question
            .alternatives
            .iter()
            .any(|alternative| alternative.recommendation.is_some());
        (door != Door::TwoWay && recommends).then_some(Self(question))
    }

    pub(crate) fn choices(&self) -> Vec<&'a Alternative> {
        let mut choices: Vec<_> = self.0.choices().collect();
        let none_of_the_above = choices.pop();
        choices.sort_by_key(|choice| Self::rank(&self.0.id, &choice.id));
        choices.extend(none_of_the_above);
        choices
    }

    pub(crate) fn offers(&self, choice: &str) -> bool {
        self.0.choice(choice).is_some()
    }

    fn rank(question: &str, choice: &str) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0100_0000_01b3;
        question
            .bytes()
            .chain([0xff])
            .chain(choice.bytes())
            .fold(OFFSET, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(PRIME)
            })
    }
}

#[derive(Default)]
pub(crate) struct FirstPicks {
    kept: Mutex<Kept>,
    changes: tokio::sync::watch::Sender<()>,
}

#[derive(Default)]
struct Kept {
    picks: VecDeque<KeptPick>,
    counts: HashMap<String, u64>,
}

struct KeptPick {
    round: String,
    question: String,
    version: u32,
    choice: String,
}

impl KeptPick {
    fn of(&self, round: &str, question: &str, version: u32) -> bool {
        self.round == round && self.question == question && self.version == version
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Pick {
    First,
    Already,
}

impl FirstPicks {
    const KEPT: usize = 64;

    pub(crate) fn of(&self, round: &str, question: &str, version: u32) -> Option<String> {
        self.lock()
            .picks
            .iter()
            .find(|kept| kept.of(round, question, version))
            .map(|kept| kept.choice.clone())
    }

    pub(crate) fn count(&self, round: Option<&str>) -> u64 {
        round
            .and_then(|round| self.lock().counts.get(round).copied())
            .unwrap_or(0)
    }

    pub(crate) fn keep(&self, round: &str, blind: &BlindQuestion<'_>, choice: &str) -> Pick {
        let question = blind.0;
        let mut kept = self.lock();
        if kept
            .picks
            .iter()
            .any(|kept| kept.of(round, &question.id, question.version))
        {
            return Pick::Already;
        }
        kept.picks.push_back(KeptPick {
            round: round.to_owned(),
            question: question.id.clone(),
            version: question.version,
            choice: choice.to_owned(),
        });
        while kept.picks.len() > Self::KEPT {
            kept.picks.pop_front();
        }
        *kept.counts.entry(round.to_owned()).or_default() += 1;
        drop(kept);
        self.changes.send_replace(());
        Pick::First
    }

    pub(crate) fn subscribe(&self) -> tokio::sync::watch::Receiver<()> {
        self.changes.subscribe()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

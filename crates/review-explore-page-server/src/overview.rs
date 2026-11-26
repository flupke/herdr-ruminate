use review_explore::{Activity, LatestTurn, QuizProgress, RoundOverview, RoundStanding};
use review_explore_page::RoundStage;

pub(crate) struct Posted<'a> {
    pub(crate) asked: usize,
    pub(crate) concluded: bool,
    pub(crate) quiz: Option<QuizProgress<'a>>,
}

impl Posted<'_> {
    pub(crate) fn overview(&self, stage: &RoundStage) -> RoundOverview {
        let activity = match stage {
            RoundStage::AgentWorking { .. } => Activity::Working,
            RoundStage::Interrupted { .. } => Activity::Interrupted,
            _ => Activity::Idle,
        };
        let latest = if self.concluded {
            LatestTurn::Conclusion { quiz: self.quiz }
        } else if self.asked > 0 {
            LatestTurn::Question
        } else {
            LatestTurn::None
        };
        RoundStanding {
            design: true,
            questions: self.asked,
            activity,
            latest,
        }
        .overview()
    }
}

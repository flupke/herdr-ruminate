use review_explore::{Comparison, EvidenceRef, InterviewUpdate};
use review_turn_path::TurnPath;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ExploreCaptured {
    pub result: Result<Arc<Comparison>, String>,
}

#[derive(Clone, Debug)]
pub struct ExploreRestored {
    pub result: Result<Option<Arc<review_explore::ExploreRound>>, String>,
    pub view: Option<review_explore::ViewSave>,
    pub historical: bool,
    pub storage_error: Option<String>,
    pub progress: ExploreProgress,
    pub implementations: Vec<ImplementationProgress>,
    pub turn_paths: BTreeMap<String, TurnPath>,
}

#[derive(Clone, Debug)]
pub struct ExploreTurnPath {
    pub round: String,
    pub request: String,
    pub path: TurnPath,
}

#[derive(Clone, Debug)]
pub struct ExploreTurnFinishing {
    pub round: String,
    pub request: String,
    pub started_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExploreProgress {
    #[default]
    Ready,
    Interrupted,
    DeliveryUncertain,
    NotStarted,
}

#[derive(Clone, Debug)]
pub struct ExplorePosted {
    pub request: review_explore::TurnRequest,
    pub result: Result<Arc<review_explore::ExploreRound>, String>,
}

#[derive(Clone, Debug)]
pub struct ExploreCommitted {
    pub round: Arc<review_explore::ExploreRound>,
    pub implementations: Vec<ImplementationProgress>,
    pub applied: bool,
    pub response: std::sync::mpsc::Sender<Result<bool, String>>,
}

#[derive(Clone, Debug)]
pub struct ExploreAnswerCancelled {
    pub answer: String,
    pub result: Result<Arc<review_explore::ExploreRound>, String>,
}

#[derive(Clone, Debug)]
pub struct ExploreStorageFailed(pub String);

#[derive(Clone, Debug)]
pub struct ExplorePageShared(pub String);

#[derive(Clone, Debug)]
pub struct ExplorePageNotShared(pub String);

#[derive(Clone, Copy, Debug)]
pub struct ExplorePageOffNetwork;

#[derive(Clone, Debug)]
pub struct ExplorePageTunnel(pub review_explore_page_tunnel::TunnelState);

#[derive(Clone, Debug)]
pub struct ExplorePageSettingsLoaded(pub review_explore_page_settings::ExplorePageSettings);

#[derive(Clone, Debug)]
pub struct ExploreRoundSettingsLoaded(pub review_explore_round_settings::ExploreRoundSettings);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExploreRunAheadUnavailable(pub Option<String>);

#[derive(Clone, Debug)]
pub struct ExplorePageNotOpened(pub review_explore_page_opening::PageNotOpened);

#[derive(Clone, Debug)]
pub struct ExplorePageStart(pub Result<(), String>);

#[derive(Clone, Debug)]
pub struct ExploreStartBlock(pub Option<review_explore::StartBlock>);

#[derive(Clone, Debug)]
pub struct ExplorePageStopped {
    pub round: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ExplorePageReset {
    pub round: String,
}

#[derive(Clone, Debug)]
pub struct ExploreImplementationSaved(pub ImplementationProgress);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplementationProgress {
    pub request: review_explore::ImplementationRequest,
    pub attempt: String,
    pub state: review_explore::ImplementationState,
}

#[derive(Clone, Debug)]
pub struct ExploreComparisonAccepted(pub Arc<Comparison>);

#[derive(Clone, Debug)]
pub struct ExploreFinished {
    pub instance: String,
    pub request: String,
    pub result: Result<InterviewUpdate, String>,
}

#[derive(Clone, Debug)]
pub struct ExploreSubmission {
    pub update: InterviewUpdate,
    pub response: std::sync::mpsc::Sender<Result<bool, String>>,
}

#[derive(Clone, Debug)]
pub struct ExploreImplementationFinished {
    pub request: review_explore::ImplementationRequest,
    pub attempt: Option<String>,
    pub state: review_explore::ImplementationState,
}

#[derive(Clone, Debug)]
pub struct ExploreEvidence {
    pub comparison: Arc<Comparison>,
    pub evidence: Vec<EvidenceRef>,
    pub view: EvidenceView,
    pub reveal: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct EvidenceView {
    pub turn: usize,
    pub reference: usize,
}

#[derive(Clone, Debug)]
pub struct ExploreViewports(pub Vec<(EvidenceView, crate::DiffViewportChanged)>);

#[derive(Clone, Debug)]
pub struct ExploreEvidenceInput {
    pub view: EvidenceView,
    pub input: crate::PointerInput,
}

#[derive(Clone, Debug)]
pub struct ExplorePositionsRestored(pub Vec<review_explore::EvidencePosition>);

#[derive(Clone, Debug)]
pub struct ExplorePositionsChanged(pub Vec<review_explore::EvidencePosition>);

#[derive(Clone, Debug)]
pub struct ExploreHistoryChanged(pub review_explore::ExploreHistory);

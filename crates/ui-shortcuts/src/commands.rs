use crate::Key;
use crate::table::{ShortcutSequence, bindings};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutCommand {
    Application(ApplicationShortcut),
    Search(SearchShortcut),
    Movement(MovementShortcut),
    Diff(DiffShortcut),
    DiffGlobal(DiffGlobalShortcut),
    Files(FilesShortcut),
    Overlay(OverlayShortcut),
    Revision(RevisionShortcut),
    Threads(ThreadsShortcut),
    Conversation(ConversationShortcut),
    Explore(ExploreShortcut),
    ExploreGlobal(ExploreGlobalShortcut),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationShortcut {
    ChangeFocus,
    OpenFiles,
    OpenThreads,
    OpenExplore,
    ToggleNavigation,
    NewReplies,
    Clear,
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchShortcut {
    Begin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementShortcut {
    MoveDown,
    MoveUp,
    GoToFirst,
    GoToLast,
    MoveHalfPageDown,
    MoveHalfPageUp,
}

impl MovementShortcut {
    pub fn row_delta(self, page: isize) -> isize {
        match self {
            Self::MoveDown => 1,
            Self::MoveUp => -1,
            Self::MoveHalfPageDown => page,
            Self::MoveHalfPageUp => -page,
            Self::GoToFirst => isize::MIN,
            Self::GoToLast => isize::MAX,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffShortcut {
    Comment(CommentShortcut),
    Location(LocationShortcut),
    SearchMatch(SearchMatchShortcut),
    Source(SourceShortcut),
    Lsp(LspShortcut),
    StartSelection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffGlobalShortcut {
    PreviousComment,
    NextComment,
    Hunk(HunkShortcut),
    ToggleHunkReviewed,
    OpenInEditor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilesShortcut {
    GoToNextUnreviewed,
    GoToPreviousUnreviewed,
    MarkReviewed,
    AutoReview,
    UnreviewAll,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverlayShortcut {
    ShowCommitMessage,
    OpenHelp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevisionShortcut {
    GoToParent,
    GoToChild,
    OpenSelector,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadsShortcut {
    ShowUnresolved,
    ShowAll,
    OpenConversation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationShortcut {
    Reply,
    ToggleResolution,
    Peek,
    MarkRead,
    FocusThreads,
    Back,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploreShortcut {
    SelectNext,
    SelectPrevious,
    ScrollDown,
    ScrollUp,
    Confirm,
    Send,
    Back,
    ChooseAnswer(usize),
    Turn(ExploreTurnShortcut),
    Evidence(ExploreEvidenceShortcut),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploreTurnShortcut {
    Start(ExploreStartShortcut),
    OpenPage,
    ContinueInPane,
    PastRounds,
    Setting(ExploreSettingShortcut),
    Cancel,
    Retry,
    PreviousTurn,
    NextTurn,
    ToggleMap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploreStartShortcut {
    Start,
    StartWithChallenger,
    StartInPane,
    StartInPaneWithChallenger,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploreSettingShortcut {
    Whose,
    WritingStyle,
    RunAhead,
    Page(ExplorePageSettingShortcut),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplorePageSettingShortcut {
    PaneStarts,
    Network,
    Interface,
    FirstPort,
    Tunnel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploreEvidenceShortcut {
    First,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploreGlobalShortcut {
    CycleFocus,
    GrowEvidence,
    ShrinkEvidence,
    FitEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommentShortcut {
    Add,
    Reply,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationShortcut {
    GoToPrevious,
    GoToNext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchMatchShortcut {
    WordUnderCursor,
    NextMatch,
    PreviousMatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HunkShortcut {
    GoToNextModified,
    GoToPreviousModified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LspShortcut {
    GoToDefinition,
    GoToTypeDefinition,
    GoToReferences,
    Restart,
    ShowDocumentation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceShortcut {
    ExpandOrMoveRight,
    MoveLeft,
    MoveToEndOfLine,
    MoveToNextWord,
    MoveToPreviousWord,
    MoveToStartOfLine,
}

pub trait ShortcutSubscription: Copy + Eq + Send + Sync + 'static {
    fn select(command: ShortcutCommand) -> Option<Self>;

    fn bound_to(key: Key) -> Option<Self> {
        bindings()
            .filter(|binding| binding.sequence == ShortcutSequence::One(key))
            .find_map(|binding| Self::select(binding.command))
    }

    fn keys(self) -> impl Iterator<Item = Key> {
        bindings().filter_map(move |binding| match binding.sequence {
            ShortcutSequence::One(key) if Self::bound_to(key) == Some(self) => Some(key),
            _ => None,
        })
    }
}

macro_rules! owner_subscription {
    ($owner:ident, $scope:ident) => {
        impl ShortcutSubscription for $owner {
            fn select(command: ShortcutCommand) -> Option<Self> {
                match command {
                    ShortcutCommand::$scope(command) => Some(command),
                    _ => None,
                }
            }
        }
    };
}

owner_subscription!(ApplicationShortcut, Application);
owner_subscription!(SearchShortcut, Search);
owner_subscription!(MovementShortcut, Movement);
owner_subscription!(DiffShortcut, Diff);
owner_subscription!(DiffGlobalShortcut, DiffGlobal);
owner_subscription!(FilesShortcut, Files);
owner_subscription!(OverlayShortcut, Overlay);
owner_subscription!(RevisionShortcut, Revision);
owner_subscription!(ThreadsShortcut, Threads);
owner_subscription!(ConversationShortcut, Conversation);
owner_subscription!(ExploreShortcut, Explore);
owner_subscription!(ExploreGlobalShortcut, ExploreGlobal);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationCommand {
    Application(ApplicationShortcut),
    Search(SearchShortcut),
}

impl ShortcutSubscription for ApplicationCommand {
    fn select(command: ShortcutCommand) -> Option<Self> {
        ApplicationShortcut::select(command)
            .map(Self::Application)
            .or_else(|| SearchShortcut::select(command).map(Self::Search))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffPaneCommand {
    Movement(MovementShortcut),
    Search(SearchShortcut),
    Diff(DiffShortcut),
}

impl ShortcutSubscription for DiffPaneCommand {
    fn select(command: ShortcutCommand) -> Option<Self> {
        MovementShortcut::select(command)
            .map(Self::Movement)
            .or_else(|| SearchShortcut::select(command).map(Self::Search))
            .or_else(|| DiffShortcut::select(command).map(Self::Diff))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadsCommand {
    Movement(MovementShortcut),
    Search(SearchShortcut),
    Threads(ThreadsShortcut),
}

impl ShortcutSubscription for ThreadsCommand {
    fn select(command: ShortcutCommand) -> Option<Self> {
        MovementShortcut::select(command)
            .map(Self::Movement)
            .or_else(|| SearchShortcut::select(command).map(Self::Search))
            .or_else(|| ThreadsShortcut::select(command).map(Self::Threads))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationCommand {
    Movement(MovementShortcut),
    Conversation(ConversationShortcut),
}

impl ShortcutSubscription for ConversationCommand {
    fn select(command: ShortcutCommand) -> Option<Self> {
        MovementShortcut::select(command)
            .map(Self::Movement)
            .or_else(|| ConversationShortcut::select(command).map(Self::Conversation))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExploreCommand {
    Global(ExploreGlobalShortcut),
    Explore(ExploreShortcut),
}

impl ShortcutSubscription for ExploreCommand {
    fn select(command: ShortcutCommand) -> Option<Self> {
        ExploreGlobalShortcut::select(command)
            .map(Self::Global)
            .or_else(|| ExploreShortcut::select(command).map(Self::Explore))
    }
}

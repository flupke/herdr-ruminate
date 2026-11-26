use std::ops::ControlFlow;

use review_threads::ThreadCommand;
use review_ui::{
    Action, DocumentAction, ExplorePageAction, LspAction, RepositoryAction, SettingsAction,
    TerminalAction,
};

pub(super) trait ActionExecutors {
    fn explore(&mut self, command: review_explore::Command) -> eyre::Result<()>;
    fn explore_page(&mut self, action: ExplorePageAction) -> eyre::Result<()>;
    fn thread(&mut self, command: ThreadCommand) -> eyre::Result<()>;
    fn document(&mut self, action: DocumentAction) -> eyre::Result<()>;
    fn lsp(&mut self, action: LspAction) -> eyre::Result<()>;
    fn settings(&mut self, action: SettingsAction) -> eyre::Result<()>;
    fn repository(&mut self, action: RepositoryAction) -> eyre::Result<()>;
    fn open_link(&mut self, url: String) -> eyre::Result<()>;
    fn terminal(&mut self, action: TerminalAction) -> eyre::Result<ControlFlow<()>>;

    fn run_all(&mut self, actions: Vec<Action>) -> eyre::Result<ControlFlow<()>> {
        for action in actions {
            if self.run(action)?.is_break() {
                return Ok(ControlFlow::Break(()));
            }
        }
        Ok(ControlFlow::Continue(()))
    }

    fn run(&mut self, action: Action) -> eyre::Result<ControlFlow<()>> {
        match action {
            Action::Explore(command) => self.explore(command)?,
            Action::ExplorePage(action) => self.explore_page(action)?,
            Action::Thread(command) => self.thread(command)?,
            Action::Document(action) => self.document(action)?,
            Action::Lsp(action) => self.lsp(action)?,
            Action::Settings(action) => self.settings(action)?,
            Action::Repository(action) => self.repository(action)?,
            Action::OpenLink(url) => self.open_link(url)?,
            Action::Terminal(action) => return self.terminal(action),
        }
        Ok(ControlFlow::Continue(()))
    }
}

#[cfg(test)]
#[path = "actions.tests.rs"]
mod tests;

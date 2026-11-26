use std::path::{Path, PathBuf};

use tokio::sync::watch;

macro_rules! asset {
    ($name:literal, $content_type:literal) => {
        Asset {
            name: $name,
            content_type: $content_type,
            body: include_bytes!(concat!("../assets/", $name)),
        }
    };
}

const SHELL: &str = include_str!("../assets/page.html");

// Browsers refuse to load a module script that is not served with a script's content type.
const ASSETS: &[Asset] = &[
    asset!("tokens.css", "text/css"),
    asset!("buttons.css", "text/css"),
    asset!("status.css", "text/css"),
    asset!("masthead.css", "text/css"),
    asset!("meter.css", "text/css"),
    asset!("style.css", "text/css"),
    asset!("conclusion.css", "text/css"),
    asset!("markdown.css", "text/css"),
    asset!("tags.css", "text/css"),
    asset!("turn.css", "text/css"),
    asset!("glossary.css", "text/css"),
    asset!("sent.css", "text/css"),
    asset!("answer-card.css", "text/css"),
    asset!("choices.css", "text/css"),
    asset!("question.css", "text/css"),
    asset!("quiz.css", "text/css"),
    asset!("citations.css", "text/css"),
    asset!("diagrams.css", "text/css"),
    asset!("design.css", "text/css"),
    asset!("earlier.css", "text/css"),
    asset!("left-column.css", "text/css"),
    asset!("past.css", "text/css"),
    asset!("swipe.css", "text/css"),
    asset!("chat.css", "text/css"),
    asset!("layout.css", "text/css"),
    asset!("favicon.svg", "image/svg+xml"),
    asset!("favicon.ico", "image/x-icon"),
    asset!("apple-touch-icon.png", "image/png"),
    asset!("icon-192.png", "image/png"),
    asset!("icon-512.png", "image/png"),
    asset!("site.webmanifest", "application/manifest+json"),
    asset!("client/main.js", "text/javascript"),
    asset!("client/actions.js", "text/javascript"),
    asset!("client/change-size.js", "text/javascript"),
    asset!("client/revision.js", "text/javascript"),
    asset!("client/chat.js", "text/javascript"),
    asset!("client/glossary.js", "text/javascript"),
    asset!("client/chat-quote.js", "text/javascript"),
    asset!("client/chat-width.js", "text/javascript"),
    asset!("client/chips.js", "text/javascript"),
    asset!("client/choices.js", "text/javascript"),
    asset!("client/citations.js", "text/javascript"),
    asset!("client/conclusion.js", "text/javascript"),
    asset!("client/disclosure.js", "text/javascript"),
    asset!("client/connection.js", "text/javascript"),
    asset!("client/design.js", "text/javascript"),
    asset!("client/desk.js", "text/javascript"),
    asset!("client/dev.js", "text/javascript"),
    asset!("client/diagrams.js", "text/javascript"),
    asset!("client/dom.js", "text/javascript"),
    asset!("client/drafts.js", "text/javascript"),
    asset!("client/answer-comment.js", "text/javascript"),
    asset!("client/drawing-board.js", "text/javascript"),
    asset!("client/earlier.js", "text/javascript"),
    asset!("client/issue-link.js", "text/javascript"),
    asset!("client/left-column.js", "text/javascript"),
    asset!("client/past.js", "text/javascript"),
    asset!("client/favicon.js", "text/javascript"),
    asset!("client/favicon-ticker.js", "text/javascript"),
    asset!("client/masthead.js", "text/javascript"),
    asset!("client/meter.js", "text/javascript"),
    asset!("client/notify.js", "text/javascript"),
    asset!("client/page.js", "text/javascript"),
    asset!("client/question.js", "text/javascript"),
    asset!("client/quiz.js", "text/javascript"),
    asset!("client/route.js", "text/javascript"),
    asset!("client/send-keys.js", "text/javascript"),
    asset!("client/socket.js", "text/javascript"),
    asset!("client/start.js", "text/javascript"),
    asset!("client/status.js", "text/javascript"),
    asset!("client/swipe.js", "text/javascript"),
    asset!("client/turn.js", "text/javascript"),
    asset!("client/sent.js", "text/javascript"),
    asset!("client/answer-card.js", "text/javascript"),
];

struct Asset {
    name: &'static str,
    content_type: &'static str,
    body: &'static [u8],
}

pub struct PageFiles(Source);

enum Source {
    Embedded,
    Disk {
        root: PathBuf,
        changes: watch::Receiver<u64>,
        _watcher: notify::RecommendedWatcher,
    },
}

impl PageFiles {
    pub fn embedded() -> Self {
        Self(Source::Embedded)
    }

    pub fn from_dir(root: PathBuf) -> notify::Result<Self> {
        let (counter, changes) = watch::channel(0);
        let watcher = watch_files(&root, counter)?;
        Ok(Self(Source::Disk {
            root,
            changes,
            _watcher: watcher,
        }))
    }

    pub(crate) fn shell(&self) -> String {
        match &self.0 {
            Source::Embedded => SHELL.replace("{dev}", ""),
            Source::Disk { root, changes, .. } => {
                let shell = std::fs::read_to_string(root.join("assets/page.html"))
                    .unwrap_or_else(|_| SHELL.to_owned());
                shell.replace("{dev}", &changes.borrow().to_string())
            }
        }
    }

    pub(crate) fn asset(&self, name: &str) -> Option<(&'static str, Vec<u8>)> {
        let asset = ASSETS.iter().find(|asset| asset.name == name)?;
        match &self.0 {
            Source::Embedded => Some((asset.content_type, asset.body.to_vec())),
            Source::Disk { root, .. } => {
                let body = std::fs::read(root.join("assets").join(asset.name)).ok()?;
                Some((asset.content_type, body))
            }
        }
    }

    pub(crate) fn changes(&self) -> Option<watch::Receiver<u64>> {
        match &self.0 {
            Source::Embedded => None,
            Source::Disk { changes, .. } => Some(changes.clone()),
        }
    }
}

fn watch_files(
    root: &Path,
    counter: watch::Sender<u64>,
) -> notify::Result<notify::RecommendedWatcher> {
    use notify::Watcher;
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|event| event.kind.is_modify() || event.kind.is_create()) {
            counter.send_modify(|count| *count += 1);
        }
    })?;
    watcher.watch(&root.join("assets"), notify::RecursiveMode::Recursive)?;
    Ok(watcher)
}

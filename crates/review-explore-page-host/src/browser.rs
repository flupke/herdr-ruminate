use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

use review_explore_page_opening::{PageNotOpened, PageScreen};

use crate::{PageDirectory, PanePlace};

#[derive(Clone, Debug)]
pub struct PageOpener {
    page: PageSource,
    browser: Browser,
}

#[derive(Clone, Debug)]
enum PageSource {
    Served(String),
    Recorded {
        pages: PageDirectory,
        place: PanePlace,
    },
}

impl PageOpener {
    pub fn new(pages: PageDirectory, place: PanePlace, browser: Browser) -> Self {
        Self {
            page: PageSource::Recorded { pages, place },
            browser,
        }
    }

    pub(crate) fn served(address: String, browser: Browser) -> Self {
        Self {
            page: PageSource::Served(address),
            browser,
        }
    }

    pub fn open(&self, screen: PageScreen) -> Result<(), PageNotOpened> {
        let address = match &self.page {
            PageSource::Served(address) => address.clone(),
            PageSource::Recorded { pages, place } => pages
                .address(place)
                .map_err(|error| PageNotOpened {
                    url: None,
                    reason: format!("Cannot read the address of the Explore page: {error}"),
                })?
                .ok_or_else(|| PageNotOpened {
                    url: None,
                    reason: "No open reviewer in this tab: open the progressive reviewer, then \
                             its Explore page"
                        .into(),
                })?,
        };
        let url = format!("{address}{}", screen.fragment());
        self.browser.open(&url).map_err(|reason| PageNotOpened {
            url: Some(url),
            reason,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Browser {
    program: OsString,
    arguments: Vec<OsString>,
}

impl Browser {
    pub fn from_env() -> Self {
        match env::var("BROWSER") {
            Ok(command) if !command.trim().is_empty() => Self::command(&command),
            _ if cfg!(target_os = "macos") => Self::command("open"),
            _ => Self::command("xdg-open"),
        }
    }

    pub fn command(command: &str) -> Self {
        let mut words = command.split_whitespace().map(OsString::from);
        Self {
            program: words.next().unwrap_or_default(),
            arguments: words.collect(),
        }
    }

    pub fn open(&self, url: &str) -> Result<(), String> {
        let program = Path::new(&self.program)
            .file_name()
            .unwrap_or(&self.program)
            .to_string_lossy();
        let status = Command::new(&self.program)
            .args(&self.arguments)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("{program} did not start: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{program} failed ({status})"))
        }
    }
}

#[cfg(test)]
#[path = "browser.tests.rs"]
mod tests;

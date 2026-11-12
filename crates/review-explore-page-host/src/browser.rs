//! Opening the page of a review pane's reviewer in the default browser of this machine: for the
//! Herdr action, by the record of its tab; for the pane's Start, Start with Challenger, Open the
//! Explore page and Past rounds, at the address its reviewer serves the page at.
//! `BROWSER` names the program and its arguments; by default `xdg-open` on Linux, `open` on
//! macOS. The page opens at the address of this machine's loopback listener, the only one that
//! offers past rounds (ADR 0003, ADR 0004).

use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

use review_explore_page_opening::{PageNotOpened, PageScreen};

use crate::{PageDirectory, PanePlace};

/// Opens the page of a review pane's reviewer, which it serves on this machine.
#[derive(Clone, Debug)]
pub struct PageOpener {
    page: PageSource,
    browser: Browser,
}

/// Where the opener finds the page's address.
#[derive(Clone, Debug)]
enum PageSource {
    /// The address that the reviewer serving the page holds: the pane's own page, whatever
    /// other reviewer runs in its tab.
    Served(String),
    /// The record of the review pane at the place, which the Herdr action reads from outside
    /// the reviewer.
    Recorded {
        pages: PageDirectory,
        place: PanePlace,
    },
}

impl PageOpener {
    /// Opens the page of the review pane at `place`, by its record in `pages`.
    pub fn new(pages: PageDirectory, place: PanePlace, browser: Browser) -> Self {
        Self {
            page: PageSource::Recorded { pages, place },
            browser,
        }
    }

    /// Opens the page at `address`, which the reviewer serves itself.
    pub(crate) fn served(address: String, browser: Browser) -> Self {
        Self {
            page: PageSource::Served(address),
            browser,
        }
    }

    /// Opens the page in the browser, on `screen`. Waits for the browser program to exit: run it
    /// off any thread that must stay responsive.
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

/// The program that opens an address in the default browser: `$BROWSER` when it is set (a
/// program and its arguments, separated by spaces), `xdg-open` on Linux, `open` on macOS.
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

    /// The program and arguments of `command`, separated by spaces, to which the address is
    /// added.
    pub fn command(command: &str) -> Self {
        let mut words = command.split_whitespace().map(OsString::from);
        Self {
            program: words.next().unwrap_or_default(),
            arguments: words.collect(),
        }
    }

    /// Opens `url`. The program's output is dropped: it would draw over the pane. A failure
    /// names the program by its file name, which the pane has room for.
    fn open(&self, url: &str) -> Result<(), String> {
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

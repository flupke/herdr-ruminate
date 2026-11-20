//! A watch on a terminal's screen: herdr-core's `terminal.attach` with `observe` on a framed connection,
//! which neither types nor resizes. It gives the screen as the core paints it first, styles
//! included, then tells each time the program prints.
//!
//! The connection turns framed after a `hello` with `framed: true`: each frame is its channel (u32,
//! little-endian), its kind (u8) and its length (u32), then its payload. Channel 0 carries the
//! JSON-RPC messages; the answer to the attach names the channel of the terminal's output, whose
//! DATA frames the watcher credits back as it reads them. herdr-core sends a new watcher a whole
//! paint at once, in frames of up to 16 KiB, then the bytes the program prints, each within a
//! millisecond or so (hc 0.1.21: median 0.5 ms; an idle Claude Code prints none).

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use herdr_client::client::{EventCanceller, Tie, read_line};
use herdr_client::protocol::PaneId;
use herdr_client::{Error, Result};
use serde::Deserialize;
use serde_json::json;

use super::{ApiError, Message, REQUEST_SEQUENCE, SOCKET_TIMEOUT, refusal, untyped};

/// The kind of a frame of the terminal's output.
const DATA: u8 = 0;
/// The kind of a frame that hands herdr-core back the room the bytes read took.
const CREDIT: u8 = 4;
/// The JSON-RPC channel.
const RPC: u32 = 0;
/// The longest frame herdr-core may send.
const FRAME_LIMIT: usize = 16 * 1024 * 1024;
/// The payload of each frame of a paint but its last: herdr-core cuts a paint into frames of
/// this size, and sends them back to back (hc 0.1.21: 185 KiB in 12 frames within a millisecond).
/// A shorter frame ends the paint; what follows it the program printed.
const PAINT_FRAME: usize = 16 * 1024;
/// How long the watcher waits for more of a paint whose last frame was full, which a paint of an
/// exact multiple of [`PAINT_FRAME`] ends with.
const PAINT_GAP: Duration = Duration::from_millis(30);
/// The size the watcher says it views: an observer never resizes the terminal, which herdr-core
/// paints at its own size.
const VIEW: (u16, u16) = (80, 24);

/// A watch on the screen of one terminal, until it drops.
pub(super) struct ScreenWatch {
    reader: BufReader<UnixStream>,
    socket_path: PathBuf,
    /// The channel of the terminal's output.
    channel: u32,
}

/// Shuts a [`ScreenWatch`]'s connection down from another thread, which ends its wait.
pub(super) struct WatchCloser(UnixStream);

impl WatchCloser {
    pub(super) fn close(&self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}

#[derive(Deserialize)]
struct Hello {
    #[serde(default)]
    framed: bool,
}

#[derive(Deserialize)]
struct Attached {
    channel: u32,
}

impl ScreenWatch {
    /// Watches the screen of `pane` on the core at `socket_path`; returns the screen as herdr-core
    /// paints it first, with its escapes, and the watch. A refusal that typed nothing, as for a
    /// terminal whose program ended, is [`Error::NotTyped`].
    pub(super) fn open(socket_path: &Path, pane: &PaneId) -> Result<(String, Self)> {
        const OPERATION: &str = "terminal.attach";
        let io = |source| Error::Io {
            operation: "watch a herdr-core terminal",
            path: socket_path.to_owned(),
            source,
        };
        let mut socket = UnixStream::connect(socket_path).map_err(io)?;
        socket.set_read_timeout(Some(SOCKET_TIMEOUT)).map_err(io)?;
        socket.set_write_timeout(Some(SOCKET_TIMEOUT)).map_err(io)?;
        let hello = json!({
            "jsonrpc": "2.0",
            "id": REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            "method": "hello",
            "params": {"framed": true},
        });
        writeln!(socket, "{hello}").map_err(io)?;
        let mut reader = BufReader::new(socket.try_clone().map_err(io)?);
        let line = read_line(&mut reader, "hello", socket_path)?;
        let hello: Message = serde_json::from_slice(&line).map_err(|source| Error::Json {
            operation: "hello",
            source,
        })?;
        match super::answer::<Hello>(hello, "hello")? {
            Ok(Hello { framed: true }) => {}
            Ok(_) => {
                return Err(Error::Protocol {
                    operation: "hello".to_owned(),
                    detail: "herdr-core did not turn the connection framed",
                });
            }
            Err(error) => return Err(refusal("hello", error)),
        }
        let id = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let attach = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": OPERATION,
            "params": {"terminal": pane.0, "observe": true, "cols": VIEW.0, "rows": VIEW.1},
        });
        write_frame(&mut socket, RPC, DATA, attach.to_string().as_bytes()).map_err(io)?;
        let mut watch = Self {
            reader,
            socket_path: socket_path.to_owned(),
            channel: 0,
        };
        let channel = loop {
            let (channel, _, payload) = watch.frame()?;
            if channel != RPC {
                continue;
            }
            let message: Message =
                serde_json::from_slice(&payload).map_err(|source| Error::Json {
                    operation: OPERATION,
                    source,
                })?;
            if message.id != Some(json!(id)) {
                continue;
            }
            match super::answer::<Attached>(message, OPERATION)? {
                Ok(attached) => break attached.channel,
                Err(error) => return Err(refused(&error)),
            }
        };
        watch.channel = channel;
        let paint = watch.paint()?;
        watch.reader.get_ref().set_read_timeout(None).map_err(io)?;
        Ok((paint, watch))
    }

    /// What closes the watch from another thread.
    pub(super) fn closer(&self) -> Result<WatchCloser> {
        self.reader
            .get_ref()
            .try_clone()
            .map(WatchCloser)
            .map_err(|source| Error::Io {
                operation: "share a herdr-core terminal's watch",
                path: self.socket_path.clone(),
                source,
            })
    }

    /// Ties the watch to `canceller` until the tie drops, so that cancelling it ends the wait of
    /// [`Self::printed`]; `None` when it was cancelled already.
    pub(super) fn tie(&self, canceller: &EventCanceller) -> Result<Option<Tie>> {
        canceller
            .tie(self.reader.get_ref())
            .map_err(|source| Error::Io {
                operation: "tie a herdr-core terminal's watch to its withdrawal",
                path: self.socket_path.clone(),
                source,
            })
    }

    /// Waits until the program prints; an error once the stream ended, as with the program, or
    /// once the watch was closed.
    pub(super) fn printed(&mut self) -> Result<()> {
        loop {
            let (channel, kind, _) = self.frame()?;
            if channel == self.channel && kind == DATA {
                return Ok(());
            }
        }
    }

    /// The first paint: the DATA frames herdr-core sends a new watcher, up to the first that is
    /// not full. A print that follows at once stays for [`Self::printed`] to tell.
    fn paint(&mut self) -> Result<String> {
        let mut paint = Vec::new();
        loop {
            let (channel, kind, payload) = self.frame()?;
            if channel != self.channel || kind != DATA {
                continue;
            }
            paint.extend_from_slice(&payload);
            if payload.len() < PAINT_FRAME || !self.more_within(PAINT_GAP) {
                return Ok(String::from_utf8_lossy(&paint).into_owned());
            }
        }
    }

    /// Whether more bytes come within `gap`.
    fn more_within(&mut self, gap: Duration) -> bool {
        if !self.reader.buffer().is_empty() {
            return true;
        }
        let socket = self.reader.get_ref();
        if socket.set_read_timeout(Some(gap)).is_err() {
            return false;
        }
        let more = self
            .reader
            .fill_buf()
            .is_ok_and(|buffered| !buffered.is_empty());
        let _ = self.reader.get_ref().set_read_timeout(Some(SOCKET_TIMEOUT));
        more
    }

    /// The next frame: its channel, its kind and its payload. A DATA frame of the terminal's
    /// output is credited back at once.
    fn frame(&mut self) -> Result<(u32, u8, Vec<u8>)> {
        let path = self.socket_path.clone();
        let io = |source| Error::Io {
            operation: "read a herdr-core terminal's watch",
            path: path.clone(),
            source,
        };
        let mut header = [0; 9];
        self.reader.read_exact(&mut header).map_err(io)?;
        let channel = u32::from_le_bytes(header[0..4].try_into().expect("four bytes"));
        let kind = header[4];
        let length = u32::from_le_bytes(header[5..9].try_into().expect("four bytes")) as usize;
        if length > FRAME_LIMIT {
            return Err(Error::Protocol {
                operation: "read a herdr-core terminal's watch".to_owned(),
                detail: "herdr-core sent a frame longer than any it sends",
            });
        }
        let mut payload = vec![0; length];
        self.reader.read_exact(&mut payload).map_err(io)?;
        if channel == self.channel && channel != RPC && kind == DATA {
            let credit = u32::try_from(length)
                .expect("a frame's length")
                .to_le_bytes();
            write_frame(self.reader.get_mut(), channel, CREDIT, &credit).map_err(io)?;
        }
        Ok((channel, kind, payload))
    }
}

/// Writes one frame to `socket`.
fn write_frame(socket: &mut UnixStream, channel: u32, kind: u8, payload: &[u8]) -> io::Result<()> {
    let length = u32::try_from(payload.len()).map_err(io::Error::other)?;
    let mut frame = Vec::with_capacity(9 + payload.len());
    frame.extend_from_slice(&channel.to_le_bytes());
    frame.push(kind);
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(payload);
    socket.write_all(&frame)
}

/// The error for herdr-core's refusal to watch a terminal: one that `terminal.send` would refuse
/// too typed nothing.
fn refused(error: &ApiError) -> Error {
    match untyped(error) {
        Some(reason) => Error::NotTyped { reason },
        None => Error::Herdr {
            operation: "terminal.attach",
            message: error.message.clone(),
        },
    }
}

#[cfg(test)]
#[path = "screen_watch.tests.rs"]
mod tests;

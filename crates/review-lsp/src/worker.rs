use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::api::{Command, Event, FileChange, Operation, Query};
use crate::manager::Manager;

pub struct Worker {
    commands: Sender<Command>,
    events: Receiver<Event>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(root: PathBuf) -> Self {
        let (commands, command_receiver) = unbounded();
        let (event_sender, events) = unbounded();
        let handle = thread::spawn(move || {
            Manager::new(root, event_sender).run(&command_receiver);
        });
        Self {
            commands,
            events,
            handle: Some(handle),
        }
    }

    pub fn open_document(&self, path: PathBuf) -> Result<(), String> {
        self.send(Command::OpenDocument(path))
    }

    pub fn request(&self, operation: Operation, query: Query) -> Result<(), String> {
        self.send(Command::Request { operation, query })
    }

    pub fn file_change_sender(&self) -> FileChangeSender {
        FileChangeSender(self.commands.clone())
    }

    pub fn restart(&self) -> Result<(), String> {
        self.send(Command::Restart)
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    pub fn event_receiver(&self) -> Receiver<Event> {
        self.events.clone()
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .send(command)
            .map_err(|_| "LSP worker stopped".to_owned())
    }
}

#[derive(Clone, Debug)]
pub struct FileChangeSender(Sender<Command>);

impl FileChangeSender {
    pub fn send(&self, changes: Vec<FileChange>) {
        let _ = self.0.send(Command::FilesChanged(changes));
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
#[path = "worker.tests.rs"]
mod tests;

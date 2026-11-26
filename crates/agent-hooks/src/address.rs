use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

// A Unix socket's address holds a path of at most 108 bytes, its closing zero included.
const ADDRESS_PATH: usize = 108;

pub(crate) fn bind(path: &Path) -> io::Result<UnixListener> {
    through_directory(path, UnixListener::bind)
}

pub(crate) fn connect(path: &Path) -> io::Result<UnixStream> {
    through_directory(path, UnixStream::connect)
}

// Linux resolves /proc/self/fd/<fd>/<name> through the directory the descriptor names.
fn through_directory<T>(path: &Path, open: impl FnOnce(PathBuf) -> io::Result<T>) -> io::Result<T> {
    let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
        return open(path.to_owned());
    };
    if path.as_os_str().len() < ADDRESS_PATH {
        return open(path.to_owned());
    }
    let directory = File::open(directory)?;
    open(
        Path::new("/proc/self/fd")
            .join(directory.as_raw_fd().to_string())
            .join(name),
    )
}

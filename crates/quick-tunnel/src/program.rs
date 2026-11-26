use std::ffi::OsString;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_fork::Wrapper;

use crate::TunnelFailure;

// cloudflared asks Cloudflare for a tunnel, then connects to it, which takes a few seconds.
const ADDRESS_WAIT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct TunnelProgram {
    pub program: OsString,
    pub wrapper: Wrapper,
    pub environment: Vec<(OsString, OsString)>,
    pub directory: PathBuf,
    pub address_wait: Duration,
}

impl TunnelProgram {
    pub fn cloudflared(wrapper: Wrapper) -> Self {
        Self {
            program: "cloudflared".into(),
            wrapper,
            environment: std::env::vars_os().collect(),
            directory: std::env::temp_dir(),
            address_wait: ADDRESS_WAIT,
        }
    }

    // Without --no-autoupdate, an update of cloudflared restarts the tunnel.
    pub(crate) fn arguments(target: SocketAddr) -> Vec<OsString> {
        ["tunnel", "--no-autoupdate", "--url"]
            .into_iter()
            .map(OsString::from)
            .chain([format!("http://{target}").into()])
            .collect()
    }

    pub(crate) fn find(&self) -> Result<PathBuf, TunnelFailure> {
        let missing = || TunnelFailure::Missing {
            program: self.program.to_string_lossy().into_owned(),
        };
        let program = Path::new(&self.program);
        if program.components().count() > 1 {
            return executable(program)
                .then(|| program.to_owned())
                .ok_or_else(missing);
        }
        let path = self
            .environment
            .iter()
            .find(|(name, _)| name == "PATH")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        std::env::split_paths(&path)
            .map(|directory| directory.join(program))
            .find(|candidate| executable(candidate))
            .ok_or_else(missing)
    }
}

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use review_mcp_config::{Client, UserConfig};

pub(super) fn install() -> eyre::Result<()> {
    let arguments = std::env::args().skip(2).collect::<Vec<_>>();
    let clients = match arguments.as_slice() {
        [] => installed_clients(),
        [client] => vec![client.parse::<Client>().map_err(eyre::Report::msg)?],
        _ => {
            eyre::bail!("usage: reviewer-control mcp-install [codex|claude|antigravity]");
        }
    };
    let bridge = std::env::current_exe()?.with_file_name("reviewer-mcp");
    let mut failed = false;
    for client in clients {
        let configuration =
            UserConfig::from_env(client, bridge.clone()).map_err(eyre::Report::msg)?;
        match configuration.install() {
            Ok(added) => eprintln!(
                "{}: {}",
                configuration.path().display(),
                if added {
                    "registered"
                } else {
                    "already configured"
                },
            ),
            Err(error) => {
                failed = true;
                eprintln!("MCP setup for {}: {error}", configuration.path().display());
            }
        }
    }
    if failed {
        eyre::bail!("Some MCP registrations could not be added; see the errors above");
    }
    Ok(())
}

fn installed_clients() -> Vec<Client> {
    let mut clients = Client::ALL.to_vec();
    for (client, label, program) in [
        (Client::Codex, "Codex", "codex"),
        (Client::Antigravity, "Antigravity", "agy"),
    ] {
        if !program_on_path(program) {
            clients.retain(|candidate| *candidate != client);
            eprintln!("{label}: no `{program}` on PATH, skipped its MCP registration");
        }
    }
    clients
}

fn program_on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| {
            fs::metadata(directory.join(program)).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
    })
}

use review_mcp::Endpoint;
use review_repository::repository::Repository;

fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let directory = std::env::current_dir().map_err(|error| error.to_string())?;
    let endpoint = match args.as_slice() {
        [] => Repository::discover(&directory)
            .map_err(|error| format!("Cannot locate the reviewer repository: {error}"))
            .and_then(|repository| Endpoint::from_env(repository.root())),
        [port] => Endpoint::for_repository(
            &directory,
            Some(port.parse::<u16>().map_err(|error| error.to_string())?),
        ),
        _ => return Err("usage: reviewer-mcp [port]".into()),
    };
    review_mcp::serve_stdio(endpoint)
}

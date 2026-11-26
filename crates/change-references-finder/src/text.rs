use change_references::Ticket;

const PREFERRED_REMOTES: [&str; 2] = ["upstream", "origin"];

const KEYWORDS: [&str; 10] = [
    "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved",
    "ticket",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitHubRepository {
    // GitHub's names ignore case.
    slug: String,
}

impl GitHubRepository {
    fn new(owner: &str, name: &str) -> Option<Self> {
        let name = name.strip_suffix(".git").unwrap_or(name);
        (is_github_name(owner) && is_github_name(name)).then(|| Self {
            slug: format!("{owner}/{name}").to_ascii_lowercase(),
        })
    }

    pub(crate) fn slug(&self) -> &str {
        &self.slug
    }

    fn owner(&self) -> &str {
        self.slug
            .split_once('/')
            .map_or(&self.slug, |(owner, _)| owner)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitHubRemotes {
    pub(crate) repository: GitHubRepository,
    pub(crate) fork_owner: Option<String>,
}

fn is_github_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub(crate) fn github_remotes(remotes: &str) -> Option<GitHubRemotes> {
    let remotes: Vec<(&str, GitHubRepository, Option<GitHubRepository>)> = remotes
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let (name, address) = (words.next()?, words.next()?);
            let push = (words.next() == Some("(push:"))
                .then(|| words.next())
                .flatten()
                .and_then(|push| repository_at(push.trim_end_matches(')')));
            Some((name, repository_at(address)?, push))
        })
        .collect();
    let (name, repository, push) = PREFERRED_REMOTES
        .iter()
        .find_map(|preferred| remotes.iter().find(|(name, ..)| name == preferred))
        .or_else(|| remotes.first())?;
    let origin = (*name == "upstream")
        .then(|| remotes.iter().find(|(name, ..)| *name == "origin"))
        .flatten()
        .map(|(_, origin, push)| push.as_ref().unwrap_or(origin));
    let fork_owner = push
        .as_ref()
        .or(origin)
        .filter(|fork| *fork != repository)
        .map(|fork| fork.owner().to_owned());
    Some(GitHubRemotes {
        repository: repository.clone(),
        fork_owner,
    })
}

fn repository_at(address: &str) -> Option<GitHubRepository> {
    let path = address.strip_prefix("git@github.com:").or_else(|| {
        let rest = address
            .strip_prefix("https://")
            .or_else(|| address.strip_prefix("ssh://"))
            .or_else(|| address.strip_prefix("git://"))?;
        let rest = rest.split_once('@').map_or(rest, |(_, host)| host);
        rest.strip_prefix("github.com/")
    })?;
    let (owner, name) = path.trim_end_matches('/').split_once('/')?;
    GitHubRepository::new(owner, name)
}

pub(crate) fn bookmark_ticket(name: &str) -> Option<u64> {
    let name = name.rsplit_once('/').map_or(name, |(_, last)| last);
    let (number, slug) = name.strip_prefix('t')?.split_once('-')?;
    if slug.is_empty() || number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

pub(crate) fn github_tickets(text: &str, home: Option<&GitHubRepository>) -> Vec<(Ticket, bool)> {
    let mut tickets = Vec::new();
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        for (index, word) in words.iter().enumerate() {
            if let Some((repository, number)) = github_issue(trim_punctuation(word)) {
                tickets.push((github_ticket(home, &repository, number), false));
                continue;
            }
            let keyword = trim_punctuation(word).to_ascii_lowercase();
            if !KEYWORDS.contains(&keyword.as_str()) {
                continue;
            }
            if let Some((repository, number)) = words.get(index + 1).and_then(|next| {
                let next = trim_punctuation(next);
                issue_reference(next, home).or_else(|| github_issue(next))
            }) {
                tickets.push((github_ticket(home, &repository, number), true));
            }
        }
    }
    tickets
}

fn issue_reference(
    reference: &str,
    home: Option<&GitHubRepository>,
) -> Option<(GitHubRepository, u64)> {
    let (repository, number) = reference.split_once('#')?;
    let number = number_of(number)?;
    let repository = if repository.is_empty() {
        home?.clone()
    } else {
        let (owner, name) = repository.split_once('/')?;
        GitHubRepository::new(owner, name)?
    };
    Some((repository, number))
}

pub(crate) fn github_issue(url: &str) -> Option<(GitHubRepository, u64)> {
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    let mut parts = path.split(['/', '#', '?']);
    let (owner, name, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if kind != "issues" {
        return None;
    }
    Some((GitHubRepository::new(owner, name)?, number_of(number)?))
}

pub(crate) fn linear_links(text: &str) -> Vec<Ticket> {
    text.split_whitespace()
        .filter_map(|word| {
            let path = trim_punctuation(word).strip_prefix("https://linear.app/")?;
            let mut parts = path.split(['/', '#', '?']);
            let (workspace, kind, id) = (parts.next()?, parts.next()?, parts.next()?);
            let (team, number) = id.split_once('-')?;
            let valid = kind == "issue"
                && !workspace.is_empty()
                && workspace
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                && !team.is_empty()
                && team.bytes().all(|byte| byte.is_ascii_alphanumeric())
                && number_of(number).is_some();
            valid.then(|| {
                let id = id.to_ascii_uppercase();
                Ticket {
                    url: format!("https://linear.app/{workspace}/issue/{id}"),
                    id,
                    title: None,
                }
            })
        })
        .collect()
}

pub(crate) fn github_ticket(
    home: Option<&GitHubRepository>,
    repository: &GitHubRepository,
    number: u64,
) -> Ticket {
    Ticket {
        id: if home == Some(repository) {
            format!("#{number}")
        } else {
            format!("{}#{number}", repository.slug())
        },
        url: format!("https://github.com/{}/issues/{number}", repository.slug()),
        title: None,
    }
}

fn trim_punctuation(word: &str) -> &str {
    let word = word.rsplit_once("](").map_or(word, |(_, link)| link);
    word.trim_matches(|c: char| {
        matches!(
            c,
            '(' | ')'
                | '['
                | ']'
                | '<'
                | '>'
                | ','
                | '.'
                | ';'
                | ':'
                | '"'
                | '\''
                | '`'
                | '*'
                | '_'
                | '!'
                | '?'
        )
    })
}

fn number_of(digits: &str) -> Option<u64> {
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

#[cfg(test)]
#[path = "text.tests.rs"]
mod tests;

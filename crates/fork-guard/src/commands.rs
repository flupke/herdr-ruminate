use super::sed;
use super::shell::Word;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    NotReading,
    // Without `--ignore-working-copy`, jj snapshots the working copy and records an operation.
    Snapshot,
}

const PLAIN: &[&str] = &[
    "rg", "grep", "ls", "cat", "head", "tail", "wc", "find", "pwd", "nl", "cut", "diff",
];
const JJ_SUBCOMMANDS: &[&[&str]] = &[
    &["log"],
    &["diff"],
    &["show"],
    &["st"],
    &["status"],
    &["evolog"],
    &["interdiff"],
    &["root"],
    &["file", "show"],
    &["file", "list"],
];
enum Global {
    Flag(&'static str),
    Valued(&'static str),
}

const JJ_GLOBALS: &[Global] = &[
    Global::Flag("--ignore-working-copy"),
    Global::Flag("--no-pager"),
    Global::Flag("--quiet"),
    Global::Valued("--at-op"),
    Global::Valued("--at-operation"),
    Global::Valued("--color"),
];
const GIT_READING_PREFIXES: &[&str] = &["--text", "--filter"];
const GIT_SUBCOMMANDS: &[&str] = &[
    "log",
    "diff",
    "show",
    "status",
    "blame",
    "ls-files",
    "cat-file",
    "rev-parse",
    "grep",
    "merge-base",
];
// git's `-c` can set the pager or a diff program, and `-C` reads another repository's
// configuration.
const GIT_GLOBALS: &[Global] = &[
    Global::Flag("--no-pager"),
    Global::Flag("-P"),
    Global::Flag("--no-optional-locks"),
];
const WRITING_OPTIONS: &[&str] = &[
    "--config",
    "--config-file",
    "--config-toml",
    "--tool",
    "--pre",
    "--hostname-bin",
    "--output",
    "--open-files-in-pager",
    "--ext-diff",
    "--textconv",
    "--filters",
    "-exec",
    "-execdir",
    "-delete",
    "-ok",
    "-okdir",
    "-fprint",
    "-fprint0",
    "-fprintf",
    "-fls",
];

pub(super) fn refusal(words: &[Word]) -> Option<Refusal> {
    let (name, arguments) = words.split_first()?;
    let reads = match name.text.as_str() {
        _ if arguments.iter().any(|word| writes(&word.text)) => false,
        "jj" => return jj_refusal(arguments),
        "git" => {
            !arguments.iter().any(|word| git_writes(&word.text))
                && subcommand(arguments, GIT_GLOBALS)
                    .is_some_and(|rest| GIT_SUBCOMMANDS.contains(&rest[0].text.as_str()))
        }
        "sed" => sed::prints_only(arguments),
        name => PLAIN.contains(&name),
    };
    (!reads).then_some(Refusal::NotReading)
}

fn writes(word: &str) -> bool {
    WRITING_OPTIONS.contains(&option_name(word))
}

// git takes a prefix of a long option for that option when no other option starts with it,
// and `git grep -O` opens the matches in a program.
fn git_writes(word: &str) -> bool {
    let name = option_name(word);
    let abbreviation = name.len() > 2
        && name.starts_with("--")
        && !GIT_READING_PREFIXES.contains(&name)
        && WRITING_OPTIONS
            .iter()
            .any(|option| option.starts_with(name));
    abbreviation || bundles(word, 'O')
}

fn bundles(word: &str, letter: char) -> bool {
    word.strip_prefix('-').is_some_and(|bundle| {
        bundle
            .trim_start_matches(|other: char| other.is_ascii_alphanumeric() && other != letter)
            .starts_with(letter)
    })
}

fn option_name(word: &str) -> &str {
    word.split_once('=').map_or(word, |(name, _)| name)
}

fn jj_refusal(arguments: &[Word]) -> Option<Refusal> {
    let reads = subcommand(arguments, JJ_GLOBALS).is_some_and(|rest| {
        JJ_SUBCOMMANDS.iter().any(|path| {
            rest.len() >= path.len() && path.iter().zip(rest).all(|(name, word)| word.text == *name)
        })
    });
    // jj takes its global options after the subcommand too, and another repository's
    // configuration can run a program.
    let repository =
        |word: &Word| bundles(&word.text, 'R') || option_name(&word.text) == "--repository";
    if !reads || arguments.iter().any(repository) {
        return Some(Refusal::NotReading);
    }
    let options = arguments.iter().take_while(|word| word.text != "--");
    (!options
        .into_iter()
        .any(|word| word.text == "--ignore-working-copy"))
    .then_some(Refusal::Snapshot)
}

fn subcommand<'a>(arguments: &'a [Word], globals: &[Global]) -> Option<&'a [Word]> {
    let mut index = 0;
    while let Some(word) = arguments.get(index) {
        if !word.text.starts_with('-') {
            return Some(&arguments[index..]);
        }
        index += match word.text.split_once('=') {
            Some((name, _)) => globals
                .iter()
                .any(|global| matches!(global, Global::Valued(valued) if *valued == name))
                .then_some(1)?,
            None => match globals.iter().find(|global| global.name() == word.text)? {
                Global::Flag(_) => 1,
                Global::Valued(_) => 2,
            },
        };
    }
    None
}

impl Global {
    fn name(&self) -> &'static str {
        match self {
            Global::Flag(name) | Global::Valued(name) => name,
        }
    }
}

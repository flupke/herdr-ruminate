use super::shell::Word;

// GNU sed takes options after its operands too, and reads `-` as standard input.
pub(super) fn prints_only(arguments: &[Word]) -> bool {
    let mut options = Options::default();
    let mut operands = Vec::new();
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        let text = word.text.as_str();
        if text == "--" {
            operands.extend(words.by_ref());
        } else if text.len() > 1 && text.starts_with('-') {
            if !options.read(text, &mut words) {
                return false;
            }
        } else {
            operands.push(word);
        }
    }
    if options.scripts.is_empty() {
        options.scripts.extend(operands.first().copied());
    }
    options.quiet
        && !options.scripts.is_empty()
        && options
            .scripts
            .iter()
            .all(|script| !script.globs && prints(&script.text))
}

#[derive(Default)]
struct Options<'a> {
    quiet: bool,
    scripts: Vec<&'a Word>,
}

impl<'a> Options<'a> {
    fn read(&mut self, text: &'a str, words: &mut impl Iterator<Item = &'a Word>) -> bool {
        match text {
            "--quiet" | "--silent" => self.quiet = true,
            "--regexp-extended" | "--separate" | "--null-data" | "--unbuffered" | "--posix"
            | "--sandbox" => {}
            "--expression" => return self.script(words.next()),
            _ => return self.bundle(&text[1..], words),
        }
        true
    }

    fn bundle(&mut self, letters: &'a str, words: &mut impl Iterator<Item = &'a Word>) -> bool {
        for (index, letter) in letters.char_indices() {
            match letter {
                'n' => self.quiet = true,
                'E' | 'r' | 's' | 'u' | 'z' => {}
                'e' => return index + 1 == letters.len() && self.script(words.next()),
                _ => return false,
            }
        }
        true
    }

    fn script(&mut self, word: Option<&'a Word>) -> bool {
        self.scripts.extend(word);
        word.is_some()
    }
}

fn prints(script: &str) -> bool {
    let mut rest = script;
    loop {
        let Some(after) = print_command(rest.trim_start()) else {
            return false;
        };
        match after.trim_start().strip_prefix(';') {
            Some(next) if next.trim().is_empty() => return true,
            Some(next) => rest = next,
            None => return after.trim().is_empty(),
        }
    }
}

fn print_command(text: &str) -> Option<&str> {
    let rest = match address(text, false) {
        Some(rest) => match rest.strip_prefix(',') {
            Some(end) => address(end, true)?,
            None => rest,
        },
        None => text,
    };
    rest.trim_start().strip_prefix('p')
}

fn address(text: &str, end: bool) -> Option<&str> {
    if let Some(regex) = text.strip_prefix('/') {
        return regex_end(regex);
    }
    if let Some(rest) = text.strip_prefix('$') {
        return Some(rest);
    }
    if end && let Some(count) = text.strip_prefix(['+', '~']) {
        return digits(count);
    }
    let rest = digits(text)?;
    Some(rest.strip_prefix('~').and_then(digits).unwrap_or(rest))
}

fn digits(text: &str) -> Option<&str> {
    let rest = text.trim_start_matches(|c: char| c.is_ascii_digit());
    (rest.len() < text.len()).then_some(rest)
}

fn regex_end(text: &str) -> Option<&str> {
    let mut chars = text.char_indices();
    while let Some((index, character)) = chars.next() {
        match character {
            '/' => return Some(&text[index + 1..]),
            '\\' => {
                chars.next()?;
            }
            _ => {}
        }
    }
    None
}

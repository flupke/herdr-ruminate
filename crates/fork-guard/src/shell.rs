// The shell-words and shlex crates treat operators as word characters and drop which
// characters were quoted, so they cannot tell `rg '|'` from `rg |`.

use std::str::Chars;

#[derive(Debug, Default)]
pub(super) struct Word {
    pub(super) text: String,
    pub(super) globs: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ShellError {
    Form(char),
    LeadingGlob(char),
    Control,
    Unclosed,
    Empty,
}

const FORMS: &[char] = &[';', '&', '>', '<', '$', '`', '(', ')', '{', '}'];
const STDERR_TARGETS: &[&str] = &["/dev/null", "&1"];
// Glob characters of Bash, and of Zsh with `EXTENDED_GLOB`.
const GLOBS: &[char] = &['*', '?', '[', '~', '^', '#'];

pub(super) fn split_pipe(command: &str) -> Result<Vec<Vec<Word>>, ShellError> {
    let mut lexer = Lexer {
        chars: command.chars(),
        commands: Vec::new(),
        words: Vec::new(),
        word: None,
        quoted: false,
    };
    while let Some(character) = lexer.chars.next() {
        lexer.unquoted(character)?;
    }
    lexer.end_command()?;
    Ok(lexer.commands)
}

struct Lexer<'a> {
    chars: Chars<'a>,
    commands: Vec<Vec<Word>>,
    words: Vec<Word>,
    word: Option<Word>,
    quoted: bool,
}

impl Lexer<'_> {
    fn unquoted(&mut self, character: char) -> Result<(), ShellError> {
        match character {
            ' ' | '\t' => self.end_word(),
            '|' => self.end_command()?,
            '>' => self.stderr_redirection()?,
            '\'' => self.single_quoted()?,
            '"' => self.double_quoted()?,
            '\\' => {
                self.quoted = true;
                let escaped = self.next_literal()?;
                self.push(escaped);
            }
            '~' if self.word.is_none() => self.push(character),
            _ if GLOBS.contains(&character) => self.glob(character)?,
            _ if FORMS.contains(&character) => return Err(ShellError::Form(character)),
            _ => self.push(non_control(character)?),
        }
        Ok(())
    }

    fn single_quoted(&mut self) -> Result<(), ShellError> {
        self.word.get_or_insert_default();
        self.quoted = true;
        loop {
            match self.next_literal()? {
                '\'' => return Ok(()),
                character => self.push(character),
            }
        }
    }

    // In Bash and Zsh double quotes, a backslash escapes only `$`, the backtick, `"` and
    // itself, and `$` or a backtick starts a substitution.
    fn double_quoted(&mut self) -> Result<(), ShellError> {
        self.word.get_or_insert_default();
        self.quoted = true;
        loop {
            match self.next_literal()? {
                '"' => return Ok(()),
                '\\' => match self.next_literal()? {
                    escaped @ ('$' | '`' | '"' | '\\') => self.push(escaped),
                    other => {
                        self.push('\\');
                        self.push(other);
                    }
                },
                // Bash and Zsh take a `$` before the closing double quote as a literal dollar sign.
                '$' if self.chars.as_str().starts_with('"') => self.push('$'),
                form @ ('$' | '`') => return Err(ShellError::Form(form)),
                character => self.push(character),
            }
        }
    }

    fn stderr_redirection(&mut self) -> Result<(), ShellError> {
        let rest = self.chars.as_str();
        let target = STDERR_TARGETS.iter().find(|target| {
            rest.strip_prefix(**target)
                .is_some_and(|after| after.is_empty() || after.starts_with([' ', '\t', '|']))
        });
        match target {
            Some(target)
                if !self.quoted && self.word.as_ref().is_some_and(|word| word.text == "2") =>
            {
                self.word = None;
                self.chars = rest[target.len()..].chars();
                Ok(())
            }
            _ => Err(ShellError::Form('>')),
        }
    }

    // Zsh expands `~[name]` with a function of the user's.
    fn glob(&mut self, character: char) -> Result<(), ShellError> {
        match &mut self.word {
            Some(word) if !word.text.is_empty() && !word.text.starts_with(['-', '~']) => {
                word.text.push(character);
                word.globs = true;
                Ok(())
            }
            _ => Err(ShellError::LeadingGlob(character)),
        }
    }

    fn next_literal(&mut self) -> Result<char, ShellError> {
        non_control(self.chars.next().ok_or(ShellError::Unclosed)?)
    }

    fn push(&mut self, character: char) {
        self.word.get_or_insert_default().text.push(character);
    }

    fn end_word(&mut self) {
        self.quoted = false;
        self.words.extend(self.word.take());
    }

    fn end_command(&mut self) -> Result<(), ShellError> {
        self.end_word();
        if self.words.is_empty() {
            return Err(ShellError::Empty);
        }
        self.commands.push(std::mem::take(&mut self.words));
        Ok(())
    }
}

fn non_control(character: char) -> Result<char, ShellError> {
    if character.is_control() {
        Err(ShellError::Control)
    } else {
        Ok(character)
    }
}

#[cfg(test)]
#[path = "shell.tests.rs"]
mod tests;

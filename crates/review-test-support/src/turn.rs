use std::path::Path;

const INDEX: &str = "index.md";
const DATA: [&str; 2] = ["round.md", "turn.md"];

pub fn with_turn(prompt: &str) -> String {
    let Some(directory) = prompt
        .split_whitespace()
        .find_map(|word| word.strip_suffix(INDEX)?.strip_suffix('/'))
    else {
        return prompt.to_owned();
    };
    let data: Option<Vec<_>> = DATA
        .iter()
        .map(|name| std::fs::read_to_string(Path::new(directory).join(name)).ok())
        .collect();
    match data {
        Some(data) => format!("{prompt}\n{}", data.join("\n")),
        None => prompt.to_owned(),
    }
}

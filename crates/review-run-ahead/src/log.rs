use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct RunAheadLog {
    path: Option<PathBuf>,
}

impl RunAheadLog {
    pub fn at(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn write(&self, line: &str) {
        let Some(path) = &self.path else {
            return;
        };
        let now = time::OffsetDateTime::now_utc();
        let line = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} UTC [pid {}] {line}\n",
            now.year(),
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
            now.millisecond(),
            std::process::id(),
        );
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

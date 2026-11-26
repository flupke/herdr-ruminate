use std::io::Read;
use std::sync::LazyLock;

#[cfg(test)]
mod tests;

macro_rules! version {
    () => {
        "11.17.2"
    };
}

pub const VERSION: &str = version!();

pub const FENCE: &str = "mermaid";

pub const FILE_NAME: &str = concat!("mermaid-", version!(), ".min.js");

pub const GZIPPED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mermaid.min.js.gz"));

pub fn script() -> &'static [u8] {
    static SCRIPT: LazyLock<Vec<u8>> = LazyLock::new(|| {
        let mut script = Vec::new();
        flate2::read::GzDecoder::new(GZIPPED)
            .read_to_end(&mut script)
            .expect("the build script wrote valid gzip");
        script
    });
    &SCRIPT
}

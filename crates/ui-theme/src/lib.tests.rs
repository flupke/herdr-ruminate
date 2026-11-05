use super::Theme;
use reviewer_config::ReviewerConfig;

fn from_config_dir(directory: &std::path::Path) -> eyre::Result<Theme> {
    Theme::from_config(&ReviewerConfig::from_dir(directory)?)
}

#[test]
fn resolves_dark_and_light_palettes() {
    assert!(Theme::resolve("catppuccin").is_some());
    assert!(Theme::resolve("catppuccin-latte").is_some());
    assert!(Theme::resolve("unknown").is_none());
}

#[test]
fn missing_plugin_config_uses_the_default() {
    let directory = tempfile::tempdir().unwrap();
    let default = Theme::resolve("catppuccin").unwrap().palette.text;

    // No config file, then a config file without a theme.
    for config in [None, Some("other = 1\n")] {
        if let Some(config) = config {
            std::fs::write(directory.path().join("config.toml"), config).unwrap();
        }
        let theme = from_config_dir(directory.path()).unwrap();
        assert_eq!(theme.palette.text, default, "{config:?}");
    }
}

#[test]
fn reads_the_palette_from_plugin_config() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("config.toml"),
        "theme = \"gruvbox\"\n",
    )
    .unwrap();

    assert_eq!(
        from_config_dir(directory.path()).unwrap().palette.text,
        Theme::resolve("gruvbox").unwrap().palette.text
    );
}

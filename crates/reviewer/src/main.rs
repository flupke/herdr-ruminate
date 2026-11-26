use reviewer::runtime::Runtime;

fn main() -> eyre::Result<()> {
    Runtime::from_env()?.run()
}

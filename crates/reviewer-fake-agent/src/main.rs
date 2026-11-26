fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    fake_agent::run(&reviewer_fake_agent::APP, &args);
}

//! The stand-in agent's program: herdr-common's fake agent with the reviewer's
//! [`stand_in_agent::APP`].

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    fake_agent::run(&stand_in_agent::APP, &args);
}

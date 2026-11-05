use super::*;

fn from(role: Role, event: Event) -> Reported {
    Reported { from: role, event }
}

fn fork(session: &str) -> Role {
    Role::fork(session)
}

fn agent() -> Role {
    Role::agent(AGENT)
}

fn started(session: &str, choice: &str) -> Reported {
    from(
        fork(session),
        Event::ForkStarted {
            parent: "agent".into(),
            prompt: format!("Explore round: r\nSelected option ID: {choice}\n"),
        },
    )
}

#[test]
fn each_fork_shows_its_choice_and_how_far_it_got() {
    let received = [
        from(fork("a"), Event::Hello { role: fork("a") }),
        started("a", "keep"),
        started("b", "change"),
        from(
            fork("a"),
            Event::Obeyed {
                command: Command::CallTool {
                    name: "mcp__herdr_reviewer__submit_question".into(),
                },
            },
        ),
        from(fork("b"), Event::TurnEnded { blocked: None }),
        from(fork("b"), Event::Disconnected),
    ];

    let forks = forks(&received);

    let summary: Vec<_> = forks
        .iter()
        .map(|fork| {
            (
                fork.session.as_str(),
                fork.choice.as_deref(),
                fork.submitted,
                fork.finished,
                fork.exited,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("a", Some("keep"), true, false, false),
            ("b", Some("change"), false, true, true),
        ]
    );
    assert_eq!(forks[0].parent, "agent");
}

#[test]
fn the_agent_s_session_is_the_one_it_started_last() {
    let started = |session: &str, start: &str| Event::SessionStarted {
        session: session.into(),
        start: start.into(),
    };
    let received = [
        from(agent(), started("own", "startup")),
        from(
            agent(),
            Event::ResumeReceived {
                session: "fork".into(),
            },
        ),
        from(agent(), started("fork", "resume")),
        // A fork's own session is no session of the agent's.
        from(fork("other"), started("other", "fork")),
        from(
            fork("other"),
            Event::ResumeReceived {
                session: "other".into(),
            },
        ),
    ];

    assert_eq!(
        agent_sessions(&received),
        AgentSessions {
            session: Some("fork".into()),
            resumed: vec!["fork".into()],
        }
    );
}

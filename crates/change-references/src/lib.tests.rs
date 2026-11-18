use super::*;

fn pull_request(number: u64, title: &str) -> PullRequest {
    PullRequest {
        number,
        title: title.to_owned(),
        url: format!("https://github.com/acme/shop/pull/{number}"),
    }
}

fn ticket(id: &str, title: Option<&str>) -> Ticket {
    Ticket {
        id: id.to_owned(),
        url: format!("https://github.com/acme/shop/issues/{}", &id[1..]),
        title: title.map(str::to_owned),
    }
}

#[test]
fn the_pull_request_names_the_change_before_its_ticket() {
    let references = ChangeReferences {
        pull_request: Some(pull_request(12, "Fix the login page")),
        tickets: vec![ticket("#7", Some("Login fails"))],
    };

    assert_eq!(references.title(), Some("Fix the login page"));
    assert_eq!(
        ChangeReferences {
            pull_request: None,
            ..references
        }
        .title(),
        Some("Login fails")
    );
}

#[test]
fn a_blank_title_names_nothing() {
    let references = ChangeReferences {
        pull_request: Some(pull_request(12, "  ")),
        tickets: vec![ticket("#7", None)],
    };

    assert_eq!(references.title(), None);
}

#[test]
fn an_offline_round_keeps_the_pull_request_an_older_round_found() {
    // Newest first: a round started offline found the ticket of its bookmark alone.
    let offline = ChangeReferences {
        pull_request: None,
        tickets: vec![ticket("#7", None)],
    };
    let online = ChangeReferences {
        pull_request: Some(pull_request(12, "Fix the login page")),
        tickets: vec![ticket("#7", Some("Login fails")), ticket("#8", None)],
    };

    let merged = ChangeReferences::of_rounds([&ChangeReferences::default(), &offline, &online]);

    assert_eq!(merged.pull_request, online.pull_request);
    // The newest round's tickets, with the title the older round read.
    assert_eq!(merged.tickets, [ticket("#7", Some("Login fails"))]);
}

#[test]
fn an_offline_round_keeps_the_ticket_title_an_older_round_read() {
    let offline = ChangeReferences {
        pull_request: None,
        tickets: vec![ticket("#7", None), ticket("#9", None)],
    };
    let online = ChangeReferences {
        pull_request: None,
        tickets: vec![ticket("#7", Some("Name past rounds"))],
    };

    let merged = ChangeReferences::of_rounds([&offline, &online]);

    assert_eq!(
        merged.tickets,
        [ticket("#7", Some("Name past rounds")), ticket("#9", None)]
    );
    assert_eq!(merged.title(), Some("Name past rounds"));
}

use super::*;

fn home() -> GitHubRepository {
    GitHubRepository::new("acme", "shop").unwrap()
}

fn ids(tickets: &[Ticket]) -> Vec<&str> {
    tickets.iter().map(|ticket| ticket.id.as_str()).collect()
}

fn said(text: &str) -> Vec<(String, bool)> {
    github_tickets(text, Some(&home()))
        .into_iter()
        .map(|(ticket, said)| (ticket.id, said))
        .collect()
}

fn github(text: &str, home: Option<&GitHubRepository>) -> Vec<Ticket> {
    github_tickets(text, home)
        .into_iter()
        .map(|(ticket, _)| ticket)
        .collect()
}

#[test]
fn reads_the_repository_of_each_form_of_github_address() {
    for address in [
        "https://github.com/acme/shop.git",
        "https://github.com/acme/shop",
        "git@github.com:acme/shop.git",
        "ssh://git@github.com/acme/shop.git",
        "https://github.com/Acme/Shop.git",
    ] {
        assert_eq!(
            github_remotes(&format!("origin {address}\n")).map(|remotes| remotes.repository.slug),
            Some("acme/shop".to_owned()),
            "{address}"
        );
    }
}

#[test]
fn reads_a_remote_that_pushes_elsewhere() {
    // As jj 0.45 lists a remote whose push address differs from its fetch address.
    let remotes = "fork git@github.com:me/shop.git\n\
                   origin https://github.com/acme/shop.git (push: git@github.com:acme/shop.git)\n";

    assert_eq!(
        github_remotes(remotes).unwrap().repository.slug,
        "acme/shop"
    );
}

#[test]
fn a_forks_clone_belongs_to_upstream_and_pushes_to_origin() {
    let remotes = "origin git@github.com:me/shop.git\n\
                   upstream https://github.com/acme/shop.git\n";

    let remotes = github_remotes(remotes).unwrap();

    assert_eq!(remotes.repository.slug, "acme/shop");
    assert_eq!(remotes.fork_owner.as_deref(), Some("me"));
}

#[test]
fn a_remote_that_pushes_to_a_fork_pushes_the_changes_branches_there() {
    let remotes = github_remotes(
        "origin https://github.com/acme/shop.git (push: git@github.com:me/shop.git)\n",
    )
    .unwrap();

    assert_eq!(remotes.repository.slug, "acme/shop");
    assert_eq!(remotes.fork_owner.as_deref(), Some("me"));
    let same = github_remotes(
        "origin https://github.com/acme/shop.git (push: git@github.com:Acme/shop.git)\n",
    )
    .unwrap();
    assert_eq!(same.fork_owner, None);
}

#[test]
fn prefers_origin_to_a_remote_listed_before_it() {
    let remotes = "archive https://github.com/acme/old-shop.git\n\
                   gitlab https://gitlab.com/acme/shop.git\n\
                   origin https://github.com/acme/shop.git\n";

    assert_eq!(
        github_remotes(remotes).unwrap().repository.slug,
        "acme/shop"
    );
}

#[test]
fn takes_the_first_github_remote_without_origin() {
    let remotes = "mirror https://gitlab.com/acme/shop.git\n\
                   archive https://github.com/acme/old-shop.git\n";

    assert_eq!(
        github_remotes(remotes).unwrap().repository.slug,
        "acme/old-shop"
    );
}

#[test]
fn a_repository_without_a_github_remote_has_none() {
    assert_eq!(
        github_remotes("origin https://gitlab.com/acme/shop.git\n"),
        None
    );
    assert_eq!(github_remotes(""), None);
}

#[test]
fn reads_herdr_bowtie_bookmarks_and_workspaces() {
    assert_eq!(bookmark_ticket("t157-name-rounds"), Some(157));
    assert_eq!(bookmark_ticket("luper/t7-name-past-rounds"), Some(7));
    for name in [
        "t-slug",
        "t157",
        "t157-",
        "tx7-slug",
        "main",
        "test-7",
        "fnd-1871-sync",
    ] {
        assert_eq!(bookmark_ticket(name), None, "{name}");
    }
}

#[test]
fn finds_issues_after_a_keyword_in_their_repository() {
    let tickets = github(
        "Name past rounds\n\nFixes #157. Ticket: other/tools#3, closes **#9**",
        Some(&home()),
    );

    assert_eq!(ids(&tickets), ["#157", "other/tools#3", "#9"]);
    assert_eq!(tickets[0].url, "https://github.com/acme/shop/issues/157");
    assert_eq!(tickets[1].url, "https://github.com/other/tools/issues/3");
}

#[test]
fn a_number_without_a_keyword_names_no_issue() {
    assert!(
        github(
            "Follow up on #12, see issue #110, refs #4, part of #9, step #2",
            Some(&home())
        )
        .is_empty()
    );
}

#[test]
fn finds_issue_links_anywhere_and_names_them_in_the_changes_repository() {
    let tickets = github(
        "See (https://github.com/acme/shop/issues/42) and \
         [that one](https://github.com/other/tools/issues/5#issuecomment-1), \
         again https://github.com/Acme/Shop/issues/42. \
         Not https://github.com/acme/shop/pull/43",
        Some(&home()),
    );

    assert_eq!(ids(&tickets), ["#42", "other/tools#5", "#42"]);
    assert_eq!(tickets[0].url, tickets[2].url);
}

#[test]
fn without_a_github_remote_only_references_that_name_their_repository_count() {
    let tickets = github(
        "Fixes #3, Ticket other/tools#4, https://github.com/acme/shop/issues/5",
        None,
    );

    assert_eq!(ids(&tickets), ["other/tools#4", "acme/shop#5"]);
}

#[test]
fn finds_linear_links_with_or_without_the_title_slug() {
    let tickets = linear_links(
        "Linear: https://linear.app/rayon/issue/fnd-1871/sync-outage and \
         <https://linear.app/rayon/issue/ENG-3>, \
         [https://linear.app/rayon/issue/OPS-4](https://linear.app/rayon/issue/OPS-4). \
         Not https://linear.app/rayon/project/x-1",
    );

    assert_eq!(ids(&tickets), ["FND-1871", "ENG-3", "OPS-4"]);
    assert_eq!(tickets[0].url, "https://linear.app/rayon/issue/FND-1871");
}

#[test]
fn only_a_keyword_on_the_same_line_says_the_change_is_for_an_issue() {
    assert_eq!(
        said(
            "Closes https://github.com/acme/shop/issues/4\nSee https://github.com/acme/shop/issues/5\nthe login fix\n#12 is unrelated"
        ),
        [
            ("#4".to_owned(), true),
            ("#4".to_owned(), false),
            ("#5".to_owned(), false),
        ]
    );
}

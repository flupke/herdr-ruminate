use super::{Candidate, ChangeEvolution};
use crate::repository::ChangeId;

fn change(id: &str) -> ChangeId {
    ChangeId::from(id.to_owned())
}

/// `own` above `a` and `b` on two lines; `a` above `c`, `c` above `d`; `b` above `own` again.
fn evolution() -> ChangeEvolution {
    let mut evolution = ChangeEvolution::of(change("own"));
    evolution.add(&change("own"), &change("a"));
    evolution.add(&change("own"), &change("b"));
    evolution.add(&change("a"), &change("c"));
    evolution.add(&change("c"), &change("d"));
    evolution.add(&change("b"), &change("own"));
    evolution
}

#[test]
fn each_line_gives_its_nearest_origin() {
    let origins = evolution().origins(|change| match change.as_str() {
        "a" | "b" | "c" => Candidate::Origin,
        _ => Candidate::Past,
    });

    assert_eq!(origins, [change("a"), change("b")]);
}

#[test]
fn a_line_goes_past_a_change_to_go_past_and_ends_at_one_that_ends_it() {
    let origins = evolution().origins(|change| match change.as_str() {
        "a" | "b" => Candidate::Past,
        "c" => Candidate::End,
        _ => Candidate::Origin,
    });

    assert_eq!(origins, []);
    let origins = evolution().origins(|change| match change.as_str() {
        "d" => Candidate::Origin,
        _ => Candidate::Past,
    });
    assert_eq!(origins, [change("d")]);
}

#[test]
fn an_evolution_reached_on_a_line_adds_its_lines() {
    let mut known = ChangeEvolution::of(change("a"));
    known.add(&change("a"), &change("c"));
    let mut evolution = ChangeEvolution::of(change("own"));
    evolution.add(&change("own"), &change("a"));

    evolution.extend(&known);

    let origins = evolution.origins(|change| match change.as_str() {
        "c" => Candidate::Origin,
        _ => Candidate::Past,
    });
    assert_eq!(origins, [change("c")]);
}

use super::*;

#[test]
fn keeps_no_variable_of_herdr_or_herdr_core() {
    let environ = b"HOME=/home/a\0HERDR_PANE_ID=w1:p1\0HERDR_SOCKET_PATH=/s\0HC_TERMINAL_ID=t7\0HC_TERMINAL_SOCKET=/c\0PATH=/bin\0";

    let names: Vec<_> = parse(environ).into_iter().map(|(name, _)| name).collect();

    assert_eq!(names, ["HOME", "PATH"]);
}

#[test]
fn keeps_the_reviewer_s_settings() {
    let environ = b"HERDR_REVIEWER_MCP_PORT=59123\0HERDR_PANE_ID=w1:p1\0";

    let names: Vec<_> = parse(environ).into_iter().map(|(name, _)| name).collect();

    assert_eq!(names, ["HERDR_REVIEWER_MCP_PORT"]);
}

#[test]
fn a_value_keeps_its_equal_signs() {
    assert_eq!(
        parse(b"OPTIONS=a=b\0"),
        [(OsString::from("OPTIONS"), OsString::from("a=b"))]
    );
}

#[test]
fn reads_the_environment_this_process_started_with() {
    let read = read(std::process::id()).unwrap();

    assert!(read.iter().any(|(name, _)| name == "PATH"));
}

#[test]
fn reads_a_name_that_starts_with_an_equal_sign_as_the_standard_library_does() {
    assert_eq!(
        parse(b"=X=1\0=lone\0\0PATH=/bin\0"),
        [
            (OsString::from("=X"), OsString::from("1")),
            (OsString::from("PATH"), OsString::from("/bin")),
        ]
    );
}

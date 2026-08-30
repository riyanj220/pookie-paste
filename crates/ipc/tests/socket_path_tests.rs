use ipc::socket_path;

#[test]
fn socket_path_ends_with_expected_name() {
    let path = socket_path();

    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some("pookie.sock"),
    );
}

#[test]
fn socket_path_contains_application_directory() {
    let path = socket_path();

    assert!(
        path.components()
            .any(|component| { component.as_os_str() == "pookie-paste" }),
    );
}

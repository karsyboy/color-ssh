use super::*;

#[test]
fn rdp_password_accepts_typed_spaces() {
    let mut app = AppState::new_for_tests();
    app.open_quick_connect_modal();

    let form = app.quick_connect.as_mut().expect("quick connect state");
    form.selected = QuickConnectField::Password;

    for ch in "secret phrase".chars() {
        app.handle_quick_connect_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }

    let form = app.quick_connect.as_ref().expect("quick connect state");
    assert_eq!(form.password.as_str().expect("valid UTF-8 password"), "secret phrase");
}

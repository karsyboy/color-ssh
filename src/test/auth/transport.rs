use super::*;

#[test]
fn askpass_prompt_classification_allows_password_prompts() {
    assert_eq!(
        classify_internal_askpass_prompt(Some("alice@example.com's password:")),
        AskpassPromptDecision::Allow
    );
    assert_eq!(classify_internal_askpass_prompt(Some("Password:")), AskpassPromptDecision::Allow);
}

#[test]
fn askpass_prompt_classification_denies_unexpected_prompts() {
    assert_eq!(
        classify_internal_askpass_prompt(Some("Enter passphrase for key '/home/me/.ssh/id_ed25519':")),
        AskpassPromptDecision::DenyUnexpected
    );
    assert_eq!(
        classify_internal_askpass_prompt(Some("Verification code:")),
        AskpassPromptDecision::DenyUnexpected
    );
    assert_eq!(
        classify_internal_askpass_prompt(Some("Enter PIN for authenticator:")),
        AskpassPromptDecision::DenyUnexpected
    );
    assert_eq!(
        classify_internal_askpass_prompt(Some("Are you sure you want to continue connecting (yes/no)?")),
        AskpassPromptDecision::DenyUnexpected
    );
    assert_eq!(classify_internal_askpass_prompt(None), AskpassPromptDecision::DenyMissing);
}

#[test]
fn configure_internal_askpass_env_uses_token_binding() {
    let mut env = Vec::new();
    configure_internal_askpass_env(&mut env, "askpass-token").expect("configure askpass env");

    assert!(env.iter().any(|(key, value)| key == INTERNAL_ASKPASS_TOKEN_ENV && value == "askpass-token"));
    assert!(!env.iter().any(|(key, _)| key == "COSSH_INTERNAL_ASKPASS_ENTRY"));
}

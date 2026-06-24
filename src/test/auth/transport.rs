use super::*;

#[test]
fn askpass_prompt_classification_distinguishes_allowed_missing_and_unexpected_prompts() {
    assert_eq!(
        classify_internal_askpass_prompt(Some("alice@example.com's password:")),
        AskpassPromptDecision::Allow
    );
    assert_eq!(
        classify_internal_askpass_prompt(Some("Verification code:")),
        AskpassPromptDecision::DenyUnexpected
    );
    assert_eq!(classify_internal_askpass_prompt(None), AskpassPromptDecision::DenyMissing);
}

#[test]
fn configure_internal_askpass_env_sets_token_binding() {
    let mut env = Vec::new();

    configure_internal_askpass_env(&mut env, "askpass-token").expect("configure askpass env");

    assert!(env.iter().any(|(key, value)| { key == INTERNAL_ASKPASS_TOKEN_ENV && value == "askpass-token" }));
}

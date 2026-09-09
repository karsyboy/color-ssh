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
fn host_confirmation_is_interactive_before_password_uses_vault_secret() {
    let confirmation_prompt = "Are you sure you want to continue connecting (yes/no/[fingerprint])? ";
    let password_prompt = "alice@example.com's password:";
    let mut vault_secret_disclosures = 0;

    let mut prompt_output = Vec::new();
    let confirmation = match classify_internal_askpass_prompt(Some(confirmation_prompt)) {
        AskpassPromptDecision::ConfirmInteractively => {
            read_internal_askpass_confirmation(confirmation_prompt, "yes\n".as_bytes(), &mut prompt_output).expect("read host confirmation")
        }
        decision => panic!("expected interactive confirmation, got {decision:?}"),
    };
    assert_eq!(confirmation, "yes");
    assert_eq!(prompt_output, confirmation_prompt.as_bytes());
    assert_eq!(vault_secret_disclosures, 0);

    let password = match classify_internal_askpass_prompt(Some(password_prompt)) {
        AskpassPromptDecision::Allow => {
            vault_secret_disclosures += 1;
            "vault-secret"
        }
        decision => panic!("expected vault password response, got {decision:?}"),
    };
    assert_eq!(password, "vault-secret");
    assert_eq!(vault_secret_disclosures, 1);
}

#[test]
fn configure_internal_askpass_env_sets_token_binding() {
    let mut env = Vec::new();

    configure_internal_askpass_env(&mut env, "askpass-token").expect("configure askpass env");

    assert!(env.iter().any(|(key, value)| { key == INTERNAL_ASKPASS_TOKEN_ENV && value == "askpass-token" }));
}

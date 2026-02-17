//! Quick-connect form state.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuickConnectField {
    User,
    Host,
    Profile,
    Logging,
    Connect,
}

impl QuickConnectField {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::User => Self::Host,
            Self::Host => Self::Profile,
            Self::Profile => Self::Logging,
            Self::Logging => Self::Connect,
            Self::Connect => Self::User,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::User => Self::Connect,
            Self::Host => Self::User,
            Self::Profile => Self::Host,
            Self::Logging => Self::Profile,
            Self::Connect => Self::Logging,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct QuickConnectState {
    pub(crate) user: String,
    pub(crate) host: String,
    pub(crate) profile_options: Vec<String>,
    pub(crate) profile_index: usize,
    pub(crate) ssh_logging: bool,
    pub(crate) selected: QuickConnectField,
    pub(crate) error: Option<String>,
}

impl QuickConnectState {
    pub(crate) fn new(default_ssh_logging: bool, mut profile_options: Vec<String>) -> Self {
        if profile_options.is_empty() {
            profile_options.push("default".to_string());
        }
        let profile_index = profile_options.iter().position(|profile| profile.eq_ignore_ascii_case("default")).unwrap_or(0);

        Self {
            user: String::new(),
            host: String::new(),
            profile_options,
            profile_index,
            ssh_logging: default_ssh_logging,
            selected: QuickConnectField::User,
            error: None,
        }
    }

    pub(crate) fn selected_profile_label(&self) -> &str {
        self.profile_options.get(self.profile_index).map(String::as_str).unwrap_or("default")
    }

    pub(crate) fn selected_profile_for_cli(&self) -> Option<String> {
        let profile = self.selected_profile_label();
        if profile.eq_ignore_ascii_case("default") {
            None
        } else {
            Some(profile.to_string())
        }
    }

    pub(crate) fn select_next_profile(&mut self) {
        if self.profile_options.is_empty() {
            return;
        }
        self.profile_index = (self.profile_index + 1) % self.profile_options.len();
    }

    pub(crate) fn select_prev_profile(&mut self) {
        if self.profile_options.is_empty() {
            return;
        }
        if self.profile_index == 0 {
            self.profile_index = self.profile_options.len() - 1;
        } else {
            self.profile_index -= 1;
        }
    }
}

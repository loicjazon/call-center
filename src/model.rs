use serde::{Deserialize, Serialize};

/// État d'un collègue tel qu'affiché dans la liste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Online,
    Offline,
    InCall,
    /// Lecture impossible (réseau, jeton, limite de débit) : jamais assimilé à « libre ».
    Unknown,
}

impl Status {
    pub fn emoji(self) -> &'static str {
        match self {
            Status::Online => "🟢",
            Status::InCall => "🔴",
            Status::Offline | Status::Unknown => "⚪",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Online => "en ligne",
            Status::InCall => "en call",
            Status::Offline => "hors ligne",
            Status::Unknown => "inconnu",
        }
    }
}

/// Membre du workspace Slack (annuaire et favoris).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub user_id: String,
    pub name: String,
}

/// Résultat d'une vérification pour un collègue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    pub user_id: String,
    pub status: Status,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_status_is_not_shown_as_online() {
        assert_eq!(Status::Unknown.label(), "inconnu");
        assert_ne!(Status::Unknown.emoji(), Status::Online.emoji());
    }
}

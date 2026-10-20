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
    /// URL de la photo de profil (absente des états enregistrés avant l'ajout des avatars).
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// Espace de travail Slack (nécessaire au lien vers la conversation).
    #[serde(default)]
    pub team_id: Option<String>,
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

    #[test]
    fn every_status_has_its_own_label_and_the_expected_pastille() {
        assert_eq!(Status::Online.label(), "en ligne");
        assert_eq!(Status::InCall.label(), "en call");
        assert_eq!(Status::Offline.label(), "hors ligne");
        assert_eq!(Status::Online.emoji(), "🟢");
        assert_eq!(Status::InCall.emoji(), "🔴");
        assert_eq!(Status::Offline.emoji(), "⚪");
    }

    #[test]
    fn member_without_avatar_url_in_old_state_files_still_deserializes() {
        let member: Member = serde_json::from_str(r#"{"user_id":"U1","name":"Alice"}"#).unwrap();

        assert_eq!(member.avatar_url, None);
    }
}

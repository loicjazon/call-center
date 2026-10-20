use crate::model::{Member, Status};
use serde_json::Value;

/// Compte système de Slack, absent de `is_bot` mais pas un collègue.
const SLACKBOT_ID: &str = "USLACKBOT";

/// Décode une réponse de l'API ; `{"ok":false,"error":…}` (même en HTTP 200) devient une erreur.
fn parse_ok(json: &str) -> Result<Value, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|error| format!("réponse illisible : {error}"))?;
    if value["ok"].as_bool() == Some(true) {
        Ok(value)
    } else {
        Err(value["error"]
            .as_str()
            .unwrap_or("erreur inconnue")
            .to_string())
    }
}

/// `users.info` : le collègue est-il en huddle ? Champ absent = pas en huddle.
pub fn parse_huddle(json: &str) -> Result<bool, String> {
    let value = parse_ok(json)?;
    Ok(value["user"]["profile"]["huddle_state"].as_str() == Some("in_a_huddle"))
}

/// `users.getPresence` : `true` si le collègue est actif.
pub fn parse_presence(json: &str) -> Result<bool, String> {
    let value = parse_ok(json)?;
    Ok(value["presence"].as_str() == Some("active"))
}

/// Le huddle prime sur la présence : un collègue en huddle mais « absent » reste en call.
pub fn status_of(is_active: bool, in_huddle: bool) -> Status {
    match (in_huddle, is_active) {
        (true, _) => Status::InCall,
        (false, true) => Status::Online,
        (false, false) => Status::Offline,
    }
}

/// Lien web qui ouvre la conversation avec un collègue dans le client Slack (navigateur).
/// `None` si l'un des identifiants n'est pas un identifiant Slack simple (lettres et chiffres) :
/// le lien est passé à `xdg-open`, rien d'autre ne doit pouvoir s'y glisser.
pub fn dm_link(team_id: &str, user_id: &str) -> Option<String> {
    let is_plain_id =
        |id: &str| !id.is_empty() && id.chars().all(|letter| letter.is_ascii_alphanumeric());
    (is_plain_id(team_id) && is_plain_id(user_id))
        .then(|| format!("https://app.slack.com/client/{team_id}/{user_id}"))
}

/// Une page de `users.list` : membres utilisables (hors bots et comptes désactivés) et curseur suivant.
pub fn parse_users_page(json: &str) -> Result<(Vec<Member>, Option<String>), String> {
    let value = parse_ok(json)?;
    let members = value["members"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|user| user["deleted"].as_bool() != Some(true))
                .filter(|user| user["is_bot"].as_bool() != Some(true))
                .filter(|user| user["id"].as_str() != Some(SLACKBOT_ID))
                .filter_map(|user| {
                    let user_id = user["id"].as_str()?.to_string();
                    let profile = &user["profile"];
                    let name = [&profile["display_name"], &profile["real_name"]]
                        .iter()
                        .filter_map(|candidate| candidate.as_str())
                        .find(|candidate| !candidate.is_empty())?
                        .to_string();
                    let avatar_url = [&profile["image_72"], &profile["image_48"]]
                        .iter()
                        .filter_map(|candidate| candidate.as_str())
                        .find(|candidate| !candidate.is_empty())
                        .map(str::to_string);
                    let team_id = user["team_id"]
                        .as_str()
                        .filter(|team| !team.is_empty())
                        .map(str::to_string);
                    Some(Member {
                        user_id,
                        name,
                        avatar_url,
                        team_id,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let cursor = value["response_metadata"]["next_cursor"]
        .as_str()
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_string);
    Ok((members, cursor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Member, Status};

    const IN_HUDDLE: &str = include_str!("../tests/fixtures/users_info_in_huddle.json");
    const FREE: &str = include_str!("../tests/fixtures/users_info_free.json");
    const NO_FIELD: &str = include_str!("../tests/fixtures/users_info_no_huddle_field.json");
    const USERS_PAGE: &str = include_str!("../tests/fixtures/users_list_page.json");
    const LAST_PAGE: &str = include_str!("../tests/fixtures/users_list_last_page.json");
    const RATE_LIMITED: &str = include_str!("../tests/fixtures/rate_limited.json");

    #[test]
    fn huddle_state_in_a_huddle_is_detected() {
        assert_eq!(parse_huddle(IN_HUDDLE), Ok(true));
        assert_eq!(parse_huddle(FREE), Ok(false));
    }

    #[test]
    fn missing_huddle_state_means_not_in_huddle() {
        assert_eq!(parse_huddle(NO_FIELD), Ok(false));
    }

    #[test]
    fn away_user_in_huddle_is_in_call() {
        assert_eq!(status_of(false, true), Status::InCall);
        assert_eq!(status_of(true, false), Status::Online);
        assert_eq!(status_of(false, false), Status::Offline);
    }

    #[test]
    fn presence_active_is_parsed() {
        assert_eq!(
            parse_presence(r#"{"ok":true,"presence":"active"}"#),
            Ok(true)
        );
        assert_eq!(
            parse_presence(r#"{"ok":true,"presence":"away"}"#),
            Ok(false)
        );
    }

    #[test]
    fn ok_false_response_is_an_error() {
        assert_eq!(parse_huddle(RATE_LIMITED), Err(String::from("ratelimited")));
        assert_eq!(
            parse_presence(RATE_LIMITED),
            Err(String::from("ratelimited"))
        );
        assert_eq!(
            parse_users_page(RATE_LIMITED),
            Err(String::from("ratelimited"))
        );
    }

    #[test]
    fn users_page_skips_bots_and_deleted_accounts() {
        let (members, cursor) = parse_users_page(USERS_PAGE).unwrap();

        assert_eq!(
            members,
            vec![
                Member {
                    user_id: "U000AAA01".into(),
                    name: "alice".into(),
                    avatar_url: Some("https://avatars.example.com/alice_72.jpg".into()),
                    team_id: Some("T000TEAM1".into())
                },
                Member {
                    user_id: "U000AAA02".into(),
                    name: "Bruno Exemple".into(),
                    avatar_url: None,
                    team_id: None
                },
            ]
        );
        assert_eq!(cursor.as_deref(), Some("dXNlcjpVMDAwQUFBMDI="));
    }

    #[test]
    fn empty_next_cursor_means_last_page() {
        let (_, cursor) = parse_users_page(LAST_PAGE).unwrap();

        assert_eq!(cursor, None);
    }

    #[test]
    fn dm_link_opens_the_conversation_with_the_colleague() {
        assert_eq!(
            dm_link("T000TEAM1", "U000AAA01"),
            Some(String::from(
                "https://app.slack.com/client/T000TEAM1/U000AAA01"
            ))
        );
    }

    #[test]
    fn dm_link_refuses_ids_that_are_not_plain_slack_ids() {
        assert_eq!(dm_link("T1&x=1", "U1"), None);
        assert_eq!(dm_link("T1", "U1 --flag"), None);
        assert_eq!(dm_link("", "U1"), None);
        assert_eq!(dm_link("T1", ""), None);
    }
}

use crate::model::{Member, Reading, Status};
use crate::slack;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

const API_BASE: &str = "https://slack.com/api";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Encodage pour une valeur de paramètre d'URL (les curseurs Slack contiennent des `=`).
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn users_info_url(user_id: &str) -> String {
    format!("{API_BASE}/users.info?user={}", encode(user_id))
}

fn users_presence_url(user_id: &str) -> String {
    format!("{API_BASE}/users.getPresence?user={}", encode(user_id))
}

fn users_list_url(cursor: Option<&str>) -> String {
    match cursor {
        Some(cursor) => format!("{API_BASE}/users.list?limit=200&cursor={}", encode(cursor)),
        None => format!("{API_BASE}/users.list?limit=200"),
    }
}

/// Client Slack en lecture seule (jeton utilisateur).
pub struct SlackClient {
    token: String,
    agent: ureq::Agent,
}

impl SlackClient {
    pub fn new(token: String) -> Self {
        let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
        Self { token, agent }
    }

    fn get(&self, url: &str) -> Result<String, String> {
        self.agent
            .get(url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .call()
            .map_err(|error| match error {
                ureq::Error::Status(429, _) => String::from("ratelimited"),
                other => other.to_string(),
            })?
            .into_string()
            .map_err(|error| error.to_string())
    }

    /// Statut d'un collègue ; toute erreur (réseau, jeton, limite de débit) donne `Unknown`, jamais « libre ».
    pub fn read_status(&self, user_id: &str) -> Reading {
        let status = self.fetch_status(user_id).unwrap_or_else(|error| {
            eprintln!("Lecture du statut de {user_id} impossible : {error}");
            Status::Unknown
        });
        Reading {
            user_id: user_id.to_string(),
            status,
        }
    }

    fn fetch_status(&self, user_id: &str) -> Result<Status, String> {
        let in_huddle = slack::parse_huddle(&self.get(&users_info_url(user_id))?)?;
        let is_active = slack::parse_presence(&self.get(&users_presence_url(user_id))?)?;
        Ok(slack::status_of(is_active, in_huddle))
    }

    /// Tous les membres utilisables du workspace (suit les pages).
    pub fn list_members(&self) -> Result<Vec<Member>, String> {
        let mut members = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let body = self.get(&users_list_url(cursor.as_deref()))?;
            let (page, next) = slack::parse_users_page(&body)?;
            members.extend(page);
            match next {
                Some(next) => cursor = Some(next),
                None => return Ok(members),
            }
        }
    }
}

/// Lance une vérification dans un thread et envoie les lectures à la boucle GTK.
pub fn spawn_poll(client: Arc<SlackClient>, user_ids: Vec<String>, sender: Sender<Vec<Reading>>) {
    std::thread::spawn(move || {
        let readings = user_ids
            .iter()
            .map(|user_id| client.read_status(user_id))
            .collect();
        // Le récepteur peut avoir disparu à l'arrêt de l'application : sans importance.
        let _ = sender.send(readings);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn users_info_url_targets_the_slack_api_with_the_user() {
        assert_eq!(
            users_info_url("U123"),
            "https://slack.com/api/users.info?user=U123"
        );
    }

    #[test]
    fn users_list_url_follows_the_cursor() {
        assert_eq!(
            users_list_url(None),
            "https://slack.com/api/users.list?limit=200"
        );
        assert_eq!(
            users_list_url(Some("abc=")),
            "https://slack.com/api/users.list?limit=200&cursor=abc%3D"
        );
    }
}

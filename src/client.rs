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

fn users_info_url(base: &str, user_id: &str) -> String {
    format!("{base}/users.info?user={}", encode(user_id))
}

fn users_presence_url(base: &str, user_id: &str) -> String {
    format!("{base}/users.getPresence?user={}", encode(user_id))
}

fn users_list_url(base: &str, cursor: Option<&str>) -> String {
    match cursor {
        Some(cursor) => format!("{base}/users.list?limit=200&cursor={}", encode(cursor)),
        None => format!("{base}/users.list?limit=200"),
    }
}

/// Client Slack en lecture seule (jeton utilisateur).
pub struct SlackClient {
    token: String,
    /// Adresse de l'API (modifiable pour les tests, qui parlent à un faux serveur local).
    base_url: String,
    agent: ureq::Agent,
}

impl SlackClient {
    pub fn new(token: String) -> Self {
        Self::with_base_url(token, API_BASE.to_string())
    }

    pub fn with_base_url(token: String, base_url: String) -> Self {
        let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
        Self {
            token,
            base_url,
            agent,
        }
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
        let in_huddle = slack::parse_huddle(&self.get(&users_info_url(&self.base_url, user_id))?)?;
        let is_active =
            slack::parse_presence(&self.get(&users_presence_url(&self.base_url, user_id))?)?;
        Ok(slack::status_of(is_active, in_huddle))
    }

    /// Tous les membres utilisables du workspace (suit les pages).
    pub fn list_members(&self) -> Result<Vec<Member>, String> {
        let mut members = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let body = self.get(&users_list_url(&self.base_url, cursor.as_deref()))?;
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
    use crate::model::Status;
    use crate::test_support::serve;
    use std::sync::mpsc;

    const IN_HUDDLE: &str = include_str!("../tests/fixtures/users_info_in_huddle.json");
    const FREE: &str = include_str!("../tests/fixtures/users_info_free.json");
    const USERS_PAGE: &str = include_str!("../tests/fixtures/users_list_page.json");
    const LAST_PAGE: &str = include_str!("../tests/fixtures/users_list_last_page.json");
    const RATE_LIMITED: &str = include_str!("../tests/fixtures/rate_limited.json");
    const ACTIVE: &str = r#"{"ok":true,"presence":"active"}"#;

    fn client_for(base_url: &str) -> SlackClient {
        SlackClient::with_base_url(String::from("xoxp-faux"), base_url.to_string())
    }

    #[test]
    fn users_info_url_targets_the_slack_api_with_the_user() {
        assert_eq!(
            users_info_url(API_BASE, "U123"),
            "https://slack.com/api/users.info?user=U123"
        );
    }

    #[test]
    fn users_list_url_follows_the_cursor() {
        assert_eq!(
            users_list_url(API_BASE, None),
            "https://slack.com/api/users.list?limit=200"
        );
        assert_eq!(
            users_list_url(API_BASE, Some("abc=")),
            "https://slack.com/api/users.list?limit=200&cursor=abc%3D"
        );
    }

    #[test]
    fn user_id_is_encoded_in_the_url() {
        assert_eq!(
            users_presence_url(API_BASE, "a b/c"),
            "https://slack.com/api/users.getPresence?user=a%20b%2Fc"
        );
    }

    #[test]
    fn colleague_in_a_huddle_is_in_call_and_the_token_is_sent() {
        let server = serve(vec![(200, IN_HUDDLE.into()), (200, ACTIVE.into())]);

        let reading = client_for(&server.base_url).read_status("U000AAA01");

        assert_eq!(reading.status, Status::InCall);
        let requests = server.requests.lock().unwrap();
        assert!(requests[0].contains("users.info?user=U000AAA01"));
        assert!(
            requests[0]
                .to_lowercase()
                .contains("authorization: bearer xoxp-faux")
        );
        assert!(requests[1].contains("users.getPresence?user=U000AAA01"));
    }

    #[test]
    fn active_colleague_without_huddle_is_online() {
        let server = serve(vec![(200, FREE.into()), (200, ACTIVE.into())]);

        let reading = client_for(&server.base_url).read_status("U000AAA02");

        assert_eq!(reading.status, Status::Online);
    }

    #[test]
    fn away_colleague_without_huddle_is_offline() {
        let away = r#"{"ok":true,"presence":"away"}"#;
        let server = serve(vec![(200, FREE.into()), (200, away.into())]);

        assert_eq!(
            client_for(&server.base_url).read_status("U1").status,
            Status::Offline
        );
    }

    #[test]
    fn ok_false_answer_gives_unknown_never_free() {
        let server = serve(vec![(200, RATE_LIMITED.into())]);

        assert_eq!(
            client_for(&server.base_url).read_status("U1").status,
            Status::Unknown
        );
    }

    #[test]
    fn http_429_gives_unknown() {
        let server = serve(vec![(429, String::new())]);

        assert_eq!(
            client_for(&server.base_url).read_status("U1").status,
            Status::Unknown
        );
    }

    #[test]
    fn unreachable_server_gives_unknown() {
        let reading = client_for("http://127.0.0.1:1").read_status("U1");

        assert_eq!(reading.status, Status::Unknown);
        assert_eq!(reading.user_id, "U1");
    }

    #[test]
    fn list_members_follows_the_pages() {
        let server = serve(vec![(200, USERS_PAGE.into()), (200, LAST_PAGE.into())]);

        let members = client_for(&server.base_url).list_members().unwrap();

        assert_eq!(members.len(), 2);
        let requests = server.requests.lock().unwrap();
        assert!(requests[1].contains("cursor=dXNlcjpVMDAwQUFBMDI%3D"));
    }

    #[test]
    fn list_members_reports_the_slack_error() {
        let server = serve(vec![(200, RATE_LIMITED.into())]);

        let error = client_for(&server.base_url).list_members().unwrap_err();

        assert_eq!(error, "ratelimited");
    }

    #[test]
    fn spawn_poll_sends_one_batch_of_readings() {
        let server = serve(vec![(200, IN_HUDDLE.into()), (200, ACTIVE.into())]);
        let (sender, receiver) = mpsc::channel();

        spawn_poll(
            Arc::new(client_for(&server.base_url)),
            vec![String::from("U000AAA01")],
            sender,
        );

        let readings = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].status, Status::InCall);
    }
}

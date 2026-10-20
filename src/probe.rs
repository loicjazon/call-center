use crate::client::SlackClient;

const API_BASE: &str = "https://slack.com/api";

/// Sonde de diagnostic : `call-center probe members` liste les identifiants,
/// `call-center probe <USER_ID>` affiche les réponses brutes de Slack pour un collègue.
pub fn run(token: &str, argument: &str) -> Result<(), String> {
    run_with_base(token, API_BASE, argument)
}

/// Même chose avec une adresse d'API explicite (les tests parlent à un faux serveur local).
fn run_with_base(token: &str, base_url: &str, argument: &str) -> Result<(), String> {
    if argument == "members" {
        let client = SlackClient::with_base_url(token.to_string(), base_url.to_string());
        for member in client.list_members()? {
            println!("{}\t{}", member.user_id, member.name);
        }
        return Ok(());
    }
    for method in ["users.info", "users.getPresence"] {
        let url = format!("{base_url}/{method}?user={argument}");
        let body = ureq::get(&url)
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(|error| error.to_string())?
            .into_string()
            .map_err(|error| error.to_string())?;
        println!("== {method}\n{body}\n");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::serve;

    #[test]
    fn probing_a_user_asks_slack_for_info_and_presence_with_the_token() {
        let server = serve(vec![
            (200, String::from(r#"{"ok":true}"#)),
            (200, String::from(r#"{"ok":true,"presence":"active"}"#)),
        ]);

        run_with_base("xoxp-faux", &server.base_url, "U1").unwrap();

        let requests = server.requests.lock().unwrap();
        assert!(requests[0].contains("users.info?user=U1"));
        assert!(requests[1].contains("users.getPresence?user=U1"));
        assert!(
            requests[0]
                .to_lowercase()
                .contains("authorization: bearer xoxp-faux")
        );
    }

    #[test]
    fn probing_members_lists_them() {
        let page = include_str!("../tests/fixtures/users_list_page.json");
        let last = include_str!("../tests/fixtures/users_list_last_page.json");
        let server = serve(vec![(200, page.into()), (200, last.into())]);

        assert!(run_with_base("xoxp-faux", &server.base_url, "members").is_ok());
    }

    #[test]
    fn probing_an_unreachable_server_is_an_error() {
        assert!(run_with_base("xoxp-faux", "http://127.0.0.1:1", "U1").is_err());
    }
}

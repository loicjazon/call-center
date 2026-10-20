use crate::client::SlackClient;

const API_BASE: &str = "https://slack.com/api";

/// Sonde de diagnostic : `call-center probe members` liste les identifiants,
/// `call-center probe <USER_ID>` affiche les réponses brutes de Slack pour un collègue.
pub fn run(token: &str, argument: &str) -> Result<(), String> {
    if argument == "members" {
        for member in SlackClient::new(token.to_string()).list_members()? {
            println!("{}\t{}", member.user_id, member.name);
        }
        return Ok(());
    }
    for method in ["users.info", "users.getPresence"] {
        let url = format!("{API_BASE}/{method}?user={argument}");
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

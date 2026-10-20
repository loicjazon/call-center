use std::path::Path;
use std::process::{Command, Stdio};

/// Nom de l'application affiché dans l'en-tête de la notification.
const APP_NAME: &str = "Call Center";

/// Texte unique de la notification de fin de call (l'avatar s'affiche à sa gauche).
pub fn summary(name: &str) -> String {
    format!("{name} est disponible 🟢")
}

/// Libellé de l'action proposée au clic sur la notification.
const OPEN_ACTION: &str = "default=Ouvrir la conversation";

/// Arguments de `notify-send` : critique (reste affichée jusqu'à fermeture), son du thème freedesktop,
/// logo de l'application en icône d'en-tête, avatar du collègue à gauche du texte (`image-path`),
/// et, si on connaît le lien, l'action « par défaut » (clic sur la notification) qui ouvre la conversation.
/// `--` protège un nom qui commencerait par un tiret.
fn command_args(
    name: &str,
    logo: Option<&Path>,
    avatar: Option<&Path>,
    link: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        format!("--app-name={APP_NAME}"),
        String::from("--urgency=critical"),
        String::from("--hint=string:sound-name:message-new-instant"),
    ];
    if let Some(logo) = logo {
        args.push(format!("--icon={}", logo.display()));
    }
    if let Some(avatar) = avatar {
        args.push(format!("--hint=string:image-path:{}", avatar.display()));
    }
    if link.is_some() {
        args.push(format!("--action={OPEN_ACTION}"));
    }
    args.extend([String::from("--"), summary(name)]);
    args
}

/// `notify-send --action` affiche le nom de l'action choisie ; seul le clic sur la notification
/// (« default ») ouvre la conversation, une simple fermeture n'affiche rien.
fn is_open_request(output: &str) -> bool {
    output.trim() == "default"
}

/// Notification de bureau via `notify-send` ; un échec est signalé sur stderr, sans interrompre l'application.
/// Lancée sans attendre sa fin, pour ne pas bloquer la boucle GTK. Si `link` est connu, un clic sur la
/// notification l'ouvre dans le navigateur (conversation avec le collègue).
pub fn send(name: &str, logo: Option<&Path>, avatar: Option<&Path>, link: Option<String>) {
    let mut command = Command::new("notify-send");
    command
        .args(command_args(name, logo, avatar, link.as_deref()))
        .stdout(Stdio::piped());
    match command.spawn() {
        // Avec une action, notify-send attend le clic ou la fermeture : un thread suit son issue.
        Ok(child) => {
            std::thread::spawn(move || {
                let Ok(output) = child.wait_with_output() else {
                    return;
                };
                if let Some(link) = link
                    && is_open_request(&String::from_utf8_lossy(&output.stdout))
                    && let Err(error) = Command::new("xdg-open").arg(&link).spawn()
                {
                    eprintln!("Ouverture de la conversation impossible (xdg-open) : {error}");
                }
            });
        }
        Err(error) => eprintln!("Notification impossible (notify-send) : {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn arguments_after_separator(args: &[String]) -> &[String] {
        let separator = args.iter().position(|arg| arg == "--").unwrap();
        &args[separator + 1..]
    }

    #[test]
    fn summary_says_the_colleague_is_available() {
        assert_eq!(summary("Alice"), "Alice est disponible 🟢");
    }

    #[test]
    fn application_is_named_call_center_with_a_space() {
        let args = command_args("Alice", None, None, None);

        assert!(args.contains(&String::from("--app-name=Call Center")));
    }

    #[test]
    fn the_text_is_the_only_line_and_does_not_repeat_the_app_name() {
        let args = command_args("Alice", None, None, None);

        assert_eq!(
            arguments_after_separator(&args),
            [String::from("Alice est disponible 🟢")]
        );
    }

    #[test]
    fn notification_stays_until_dismissed_and_plays_a_sound() {
        let args = command_args("Alice", None, None, None);

        assert!(args.contains(&String::from("--urgency=critical")));
        assert!(args.contains(&String::from(
            "--hint=string:sound-name:message-new-instant"
        )));
    }

    #[test]
    fn logo_is_the_notification_icon() {
        let args = command_args("Alice", Some(Path::new("/tmp/logo.svg")), None, None);

        assert!(args.contains(&String::from("--icon=/tmp/logo.svg")));
    }

    #[test]
    fn avatar_is_shown_next_to_the_text() {
        let args = command_args("Alice", None, Some(Path::new("/tmp/U1-abc")), None);

        assert!(args.contains(&String::from("--hint=string:image-path:/tmp/U1-abc")));
        assert!(!args.iter().any(|arg| arg.starts_with("--icon")));
    }

    #[test]
    fn no_icon_or_image_option_without_logo_and_avatar() {
        let args = command_args("Alice", None, None, None);

        assert!(!args.iter().any(|arg| arg.starts_with("--icon")));
        assert!(!args.iter().any(|arg| arg.contains("image-path")));
    }

    #[test]
    fn name_starting_with_a_dash_is_not_read_as_an_option() {
        let args = command_args("-Alice", None, None, None);

        assert_eq!(
            arguments_after_separator(&args),
            [String::from("-Alice est disponible 🟢")]
        );
    }

    #[test]
    fn notification_offers_to_open_the_conversation_when_a_link_is_known() {
        let args = command_args(
            "Alice",
            None,
            None,
            Some("https://app.slack.com/client/T1/U1"),
        );

        assert!(args.contains(&String::from("--action=default=Ouvrir la conversation")));
    }

    #[test]
    fn notification_has_no_action_without_a_link() {
        let args = command_args("Alice", None, None, None);

        assert!(!args.iter().any(|arg| arg.starts_with("--action")));
    }

    #[test]
    fn only_the_default_action_opens_the_conversation() {
        assert!(is_open_request("default\n"));
        assert!(is_open_request("default"));
        assert!(!is_open_request(""));
        assert!(!is_open_request("autre\n"));
    }
}

use std::path::Path;
use std::process::Command;

/// Nom de l'application affiché dans l'en-tête de la notification.
const APP_NAME: &str = "Call Center";

/// Texte unique de la notification de fin de call (l'avatar s'affiche à sa gauche).
pub fn summary(name: &str) -> String {
    format!("{name} est disponible 🟢")
}

/// Arguments de `notify-send` : critique (reste affichée jusqu'à fermeture), son du thème freedesktop,
/// logo de l'application en icône d'en-tête, avatar du collègue à gauche du texte (`image-path`).
/// `--` protège un nom qui commencerait par un tiret.
fn command_args(name: &str, logo: Option<&Path>, avatar: Option<&Path>) -> Vec<String> {
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
    args.extend([String::from("--"), summary(name)]);
    args
}

/// Notification de bureau via `notify-send` ; un échec est signalé sur stderr, sans interrompre l'application.
/// Lancée sans attendre sa fin, pour ne pas bloquer la boucle GTK.
pub fn send(name: &str, logo: Option<&Path>, avatar: Option<&Path>) {
    match Command::new("notify-send")
        .args(command_args(name, logo, avatar))
        .spawn()
    {
        // Le processus se termine seul ; `wait` dans un thread évite de laisser un zombie.
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
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
        let args = command_args("Alice", None, None);

        assert!(args.contains(&String::from("--app-name=Call Center")));
    }

    #[test]
    fn the_text_is_the_only_line_and_does_not_repeat_the_app_name() {
        let args = command_args("Alice", None, None);

        assert_eq!(
            arguments_after_separator(&args),
            [String::from("Alice est disponible 🟢")]
        );
    }

    #[test]
    fn notification_stays_until_dismissed_and_plays_a_sound() {
        let args = command_args("Alice", None, None);

        assert!(args.contains(&String::from("--urgency=critical")));
        assert!(args.contains(&String::from(
            "--hint=string:sound-name:message-new-instant"
        )));
    }

    #[test]
    fn logo_is_the_notification_icon() {
        let args = command_args("Alice", Some(Path::new("/tmp/logo.svg")), None);

        assert!(args.contains(&String::from("--icon=/tmp/logo.svg")));
    }

    #[test]
    fn avatar_is_shown_next_to_the_text() {
        let args = command_args("Alice", None, Some(Path::new("/tmp/U1-abc")));

        assert!(args.contains(&String::from("--hint=string:image-path:/tmp/U1-abc")));
        assert!(!args.iter().any(|arg| arg.starts_with("--icon")));
    }

    #[test]
    fn no_icon_or_image_option_without_logo_and_avatar() {
        let args = command_args("Alice", None, None);

        assert!(!args.iter().any(|arg| arg.starts_with("--icon")));
        assert!(!args.iter().any(|arg| arg.contains("image-path")));
    }

    #[test]
    fn name_starting_with_a_dash_is_not_read_as_an_option() {
        let args = command_args("-Alice", None, None);

        assert_eq!(
            arguments_after_separator(&args),
            [String::from("-Alice est disponible 🟢")]
        );
    }
}

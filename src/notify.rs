use std::path::Path;
use std::process::Command;

/// Titre et corps de la notification de fin de call.
pub fn message(name: &str) -> (String, String) {
    (
        String::from("call-center"),
        format!("{name} est disponible 🟢"),
    )
}

/// Arguments de `notify-send` : critique (reste affichée jusqu'à fermeture), son du thème freedesktop,
/// avatar en icône quand il est en cache. `--` protège un nom qui commencerait par un tiret.
fn command_args(name: &str, icon: Option<&Path>) -> Vec<String> {
    let (title, body) = message(name);
    let mut args = vec![
        String::from("--app-name=call-center"),
        String::from("--urgency=critical"),
        String::from("--hint=string:sound-name:message-new-instant"),
    ];
    if let Some(icon) = icon {
        args.push(format!("--icon={}", icon.display()));
    }
    args.extend([String::from("--"), title, body]);
    args
}

/// Notification de bureau via `notify-send` ; un échec est signalé sur stderr, sans interrompre l'application.
/// Lancée sans attendre sa fin, pour ne pas bloquer la boucle GTK.
pub fn send(name: &str, icon: Option<&Path>) {
    match Command::new("notify-send")
        .args(command_args(name, icon))
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

    #[test]
    fn message_says_the_colleague_is_available() {
        let (title, body) = message("Alice");

        assert_eq!(title, "call-center");
        assert_eq!(body, "Alice est disponible 🟢");
    }

    #[test]
    fn notification_stays_until_dismissed_and_plays_a_sound() {
        let args = command_args("Alice", None);

        assert!(args.contains(&String::from("--urgency=critical")));
        assert!(args.contains(&String::from(
            "--hint=string:sound-name:message-new-instant"
        )));
    }

    #[test]
    fn notification_uses_the_avatar_as_icon_when_available() {
        let args = command_args("Alice", Some(Path::new("/tmp/U1-abc")));

        assert!(args.contains(&String::from("--icon=/tmp/U1-abc")));
    }

    #[test]
    fn notification_has_no_icon_option_without_an_avatar() {
        let args = command_args("Alice", None);

        assert!(!args.iter().any(|arg| arg.starts_with("--icon")));
    }

    #[test]
    fn name_starting_with_a_dash_is_not_read_as_an_option() {
        let args = command_args("-Alice", None);

        let separator = args.iter().position(|arg| arg == "--").unwrap();
        assert_eq!(args[separator + 1], "call-center");
        assert_eq!(args[separator + 2], "-Alice est disponible 🟢");
    }
}

use std::process::Command;

/// Titre et corps de la notification de fin de call.
pub fn message(name: &str) -> (String, String) {
    (
        String::from("call-center"),
        format!("{name} n'est plus en call"),
    )
}

/// Notification de bureau via `notify-send` ; un échec est signalé sur stderr, sans interrompre l'application.
pub fn send(name: &str) {
    let (title, body) = message(name);
    if let Err(error) = Command::new("notify-send")
        .args(["--app-name=call-center", &title, &body])
        .status()
    {
        eprintln!("Notification impossible (notify-send) : {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_names_the_colleague() {
        let (title, body) = message("Alice");

        assert_eq!(title, "call-center");
        assert_eq!(body, "Alice n'est plus en call");
    }
}

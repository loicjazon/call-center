use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Intervalle entre deux vérifications du statut des collègues (5 min).
pub const CHECK_INTERVAL_SECONDS: u32 = 300;
/// Première vérification, juste après le démarrage de la boucle GTK.
pub const FIRST_CHECK_SECONDS: u32 = 1;

/// Dossier de configuration (`$XDG_CONFIG_HOME/call-center`, par défaut `~/.config/call-center`).
pub fn config_dir() -> Option<PathBuf> {
    let config_home = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(config_home.join("call-center"))
}

/// Lit le jeton Slack ; refuse un fichier lisible par d'autres que son propriétaire.
pub fn read_token(dir: &Path) -> Result<String, String> {
    let file = dir.join("token");
    let metadata = fs::metadata(&file).map_err(|_| {
        format!(
            "Jeton Slack introuvable : placez-le dans {} (chmod 600)",
            file.display()
        )
    })?;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "{} est lisible par d'autres utilisateurs : faites `chmod 0600` dessus",
            file.display()
        ));
    }
    let token = fs::read_to_string(&file)
        .map_err(|error| format!("Lecture du jeton impossible : {error}"))?;
    let token = token.trim();
    if token.is_empty() {
        return Err(format!("Le fichier {} est vide", file.display()));
    }
    Ok(token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn dir_with_token(tag: &str, content: &str, mode: u32) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("call-center-token-{}-{tag}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("token");
        fs::write(&file, content).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(mode)).unwrap();
        dir
    }

    #[test]
    fn token_with_loose_permissions_is_rejected() {
        let dir = dir_with_token("loose", "xoxp-faux", 0o644);

        let result = read_token(&dir);

        assert!(result.unwrap_err().contains("0600"));
    }

    #[test]
    fn token_with_strict_permissions_is_read_and_trimmed() {
        let dir = dir_with_token("strict", "xoxp-faux\n", 0o600);

        assert_eq!(read_token(&dir), Ok(String::from("xoxp-faux")));
    }

    #[test]
    fn missing_token_explains_where_to_put_it() {
        let dir = std::env::temp_dir().join("call-center-no-token");

        let error = read_token(&dir).unwrap_err();

        assert!(error.contains("token"), "{error}");
    }

    #[test]
    fn empty_token_is_rejected() {
        let dir = dir_with_token("empty", "  \n", 0o600);

        assert!(read_token(&dir).is_err());
    }
}

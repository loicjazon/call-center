use crate::model::Member;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;
use std::{env, fs};

/// Taille maximale acceptée pour une photo.
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Dossier du cache d'images (`$XDG_CACHE_HOME/call-center/avatars`, par défaut `~/.cache/...`).
pub fn cache_dir() -> Option<PathBuf> {
    let cache_home = match env::var_os("XDG_CACHE_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from(env::var_os("HOME")?).join(".cache"),
    };
    Some(cache_home.join("call-center/avatars"))
}

/// Empreinte FNV-1a 64 bits : change quand l'URL (donc la photo) change.
fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Nom de fichier du cache : identifiant (nettoyé) + empreinte de l'URL. Jamais de séparateur de chemin.
pub fn file_name(user_id: &str, url: &str) -> String {
    let safe_id: String = user_id
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() {
                letter
            } else {
                '_'
            }
        })
        .collect();
    format!("{safe_id}-{:016x}", fingerprint(url))
}

/// Seules les URL `https` sont téléchargées.
pub fn is_downloadable(url: &str) -> bool {
    url.strip_prefix("https://")
        .is_some_and(|rest| !rest.is_empty())
}

/// Chemin de la photo en cache pour ce collègue, si une URL est connue.
pub fn path_for(dir: &Path, member: &Member) -> Option<PathBuf> {
    let url = member
        .avatar_url
        .as_deref()
        .filter(|url| is_downloadable(url))?;
    Some(dir.join(file_name(&member.user_id, url)))
}

/// Télécharge une photo. Requête volontairement **sans** l'en-tête `Authorization` : le jeton Slack
/// ne doit jamais partir vers un serveur d'images.
fn download(url: &str, destination: &Path) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
    let response = agent.get(url).call().map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let tmp = destination.with_extension("tmp");
    fs::write(&tmp, &bytes).map_err(|error| error.to_string())?;
    fs::rename(&tmp, destination).map_err(|error| error.to_string())
}

/// Télécharge dans un thread les photos qui ne sont pas encore en cache, puis prévient une fois
/// (`done`) pour que l'interface se redessine.
pub fn spawn_downloads(dir: PathBuf, members: Vec<Member>, done: Sender<()>) {
    let jobs: Vec<(PathBuf, String)> = members
        .iter()
        .filter_map(|member| {
            let path = path_for(&dir, member)?;
            let url = member.avatar_url.clone()?;
            (!path.exists()).then_some((path, url))
        })
        .collect();
    if jobs.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        if let Err(error) = fs::create_dir_all(&dir) {
            eprintln!("Cache des avatars indisponible : {error}");
            return;
        }
        let mut fetched = false;
        for (path, url) in jobs {
            match download(&url, &path) {
                Ok(()) => fetched = true,
                Err(error) => eprintln!("Avatar non téléchargé : {error}"),
            }
        }
        if fetched {
            let _ = done.send(());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_changes_when_the_photo_changes() {
        let before = file_name("U1", "https://avatars.example.com/a_72.jpg");
        let after = file_name("U1", "https://avatars.example.com/b_72.jpg");

        assert_ne!(before, after);
        assert!(before.starts_with("U1-"));
    }

    #[test]
    fn file_name_is_stable_for_the_same_photo() {
        assert_eq!(
            file_name("U1", "https://avatars.example.com/a_72.jpg"),
            file_name("U1", "https://avatars.example.com/a_72.jpg")
        );
    }

    #[test]
    fn file_name_cannot_escape_the_cache_directory() {
        let name = file_name("../../etc", "https://avatars.example.com/a.jpg");

        assert!(!name.contains('/'), "{name}");
        assert!(!name.contains(".."), "{name}");
    }

    #[test]
    fn only_https_urls_are_downloaded() {
        assert!(is_downloadable("https://avatars.slack-edge.com/x.jpg"));
        assert!(!is_downloadable("http://avatars.slack-edge.com/x.jpg"));
        assert!(!is_downloadable("file:///etc/passwd"));
        assert!(!is_downloadable(""));
    }
}

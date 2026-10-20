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

/// Photos à télécharger : celles qui ont une URL `https` et ne sont pas déjà en cache.
fn missing_jobs(dir: &Path, members: &[Member]) -> Vec<(PathBuf, String)> {
    members
        .iter()
        .filter_map(|member| {
            let path = path_for(dir, member)?;
            let url = member.avatar_url.clone()?;
            (!path.exists()).then_some((path, url))
        })
        .collect()
}

/// Télécharge les photos (en créant le dossier du cache) ; `true` si au moins une est arrivée.
fn fetch_jobs(dir: &Path, jobs: Vec<(PathBuf, String)>) -> bool {
    if let Err(error) = fs::create_dir_all(dir) {
        eprintln!("Cache des avatars indisponible : {error}");
        return false;
    }
    let mut fetched = false;
    for (path, url) in jobs {
        match download(&url, &path) {
            Ok(()) => fetched = true,
            Err(error) => eprintln!("Avatar non téléchargé : {error}"),
        }
    }
    fetched
}

/// Télécharge dans un thread les photos qui ne sont pas encore en cache, puis prévient une fois
/// (`done`) pour que l'interface se redessine.
pub fn spawn_downloads(dir: PathBuf, members: Vec<Member>, done: Sender<()>) {
    let jobs = missing_jobs(&dir, &members);
    if jobs.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        if fetch_jobs(&dir, jobs) {
            let _ = done.send(());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::serve;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    fn temp_dir() -> std::path::PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "call-center-avatar-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn member(user_id: &str, avatar_url: Option<&str>) -> Member {
        Member {
            user_id: user_id.to_string(),
            name: String::from("Alice"),
            avatar_url: avatar_url.map(str::to_string),
            team_id: None,
        }
    }

    #[test]
    fn path_for_is_inside_the_cache_directory_for_https_urls() {
        let dir = Path::new("/cache");
        let path = path_for(
            dir,
            &member("U1", Some("https://avatars.example.com/a.jpg")),
        )
        .unwrap();

        assert_eq!(
            path,
            dir.join(file_name("U1", "https://avatars.example.com/a.jpg"))
        );
    }

    #[test]
    fn path_for_is_none_without_a_downloadable_url() {
        let dir = Path::new("/cache");

        assert_eq!(path_for(dir, &member("U1", None)), None);
        assert_eq!(
            path_for(dir, &member("U1", Some("http://example.com/a.jpg"))),
            None
        );
    }

    #[test]
    fn missing_jobs_skips_photos_already_in_cache_and_members_without_url() {
        let dir = temp_dir();
        let cached = member("U1", Some("https://avatars.example.com/a.jpg"));
        fs::write(path_for(&dir, &cached).unwrap(), b"deja la").unwrap();
        let members = vec![
            cached,
            member("U2", Some("https://avatars.example.com/b.jpg")),
            member("U3", None),
        ];

        let jobs = missing_jobs(&dir, &members);

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].1, "https://avatars.example.com/b.jpg");
    }

    #[test]
    fn download_writes_the_file_without_sending_the_slack_token() {
        let server = serve(vec![(200, String::from("octets-de-limage"))]);
        let destination = temp_dir().join("U1-photo");

        download(&format!("{}/photo.jpg", server.base_url), &destination).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"octets-de-limage");
        let request = server.requests.lock().unwrap()[0].to_lowercase();
        assert!(!request.contains("authorization"), "{request}");
    }

    #[test]
    fn download_failure_leaves_no_file() {
        let server = serve(vec![(404, String::from("introuvable"))]);
        let destination = temp_dir().join("U1-photo");

        let result = download(&format!("{}/photo.jpg", server.base_url), &destination);

        assert!(result.is_err());
        assert!(!destination.exists());
    }

    #[test]
    fn download_is_capped_to_the_maximum_size() {
        let server = serve(vec![(200, "x".repeat(MAX_BYTES as usize + 1000))]);
        let destination = temp_dir().join("U1-photo");

        download(&format!("{}/photo.jpg", server.base_url), &destination).unwrap();

        assert_eq!(fs::metadata(&destination).unwrap().len(), MAX_BYTES);
    }

    #[test]
    fn fetch_jobs_creates_the_directory_and_reports_success() {
        let server = serve(vec![(200, String::from("photo"))]);
        let dir = temp_dir().join("sous-dossier");
        let target = dir.join("U1-photo");

        let fetched = fetch_jobs(
            &dir,
            vec![(target.clone(), format!("{}/a.jpg", server.base_url))],
        );

        assert!(fetched);
        assert_eq!(fs::read(&target).unwrap(), b"photo");
    }

    #[test]
    fn fetch_jobs_reports_nothing_fetched_when_every_download_fails() {
        let server = serve(vec![(500, String::new())]);
        let dir = temp_dir();

        let fetched = fetch_jobs(
            &dir,
            vec![(dir.join("U1-photo"), format!("{}/a.jpg", server.base_url))],
        );

        assert!(!fetched);
    }
}

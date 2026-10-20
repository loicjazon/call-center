use crate::model::Member;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const STATE_FILE: &str = "state.json";
const MEMBERS_FILE: &str = "users-cache.json";
/// Au-delà, l'annuaire mis en cache est rafraîchi.
const CACHE_MAX_AGE: Duration = Duration::from_secs(24 * 3600);

/// Ce qui survit à un redémarrage : favoris et cloches armées.
#[derive(Default, Serialize, Deserialize)]
pub struct State {
    pub favorites: Vec<Member>,
    pub armed: HashSet<String>,
}

/// Écriture atomique : fichier temporaire dans le même dossier, puis renommage.
fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    let dir = path.parent().ok_or("chemin sans dossier")?;
    fs::create_dir_all(dir).map_err(|error| format!("création de {} : {error}", dir.display()))?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).map_err(|error| format!("écriture de {} : {error}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|error| format!("renommage de {} : {error}", tmp.display()))
}

/// Charge l'état ; absent ou vide → état vide. Un fichier illisible est conservé en `.bak`, jamais écrasé.
pub fn load(dir: &Path) -> State {
    let path = dir.join(STATE_FILE);
    let Ok(content) = fs::read_to_string(&path) else {
        return State::default();
    };
    if content.trim().is_empty() {
        return State::default();
    }
    serde_json::from_str(&content).unwrap_or_else(|error| {
        let backup = path.with_extension("json.bak");
        eprintln!(
            "{} illisible ({error}) : conservé dans {}",
            path.display(),
            backup.display()
        );
        let _ = fs::rename(&path, &backup);
        State::default()
    })
}

pub fn save(dir: &Path, state: &State) -> Result<(), String> {
    let content = serde_json::to_string_pretty(state).map_err(|error| error.to_string())?;
    write_atomic(&dir.join(STATE_FILE), &content)
}

/// Annuaire mis en cache, avec la date de dernière écriture.
pub fn load_members(dir: &Path) -> Option<(Vec<Member>, SystemTime)> {
    let path = dir.join(MEMBERS_FILE);
    let modified = fs::metadata(&path).ok()?.modified().ok()?;
    let members = serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()?;
    Some((members, modified))
}

pub fn save_members(dir: &Path, members: &[Member]) -> Result<(), String> {
    let content = serde_json::to_string(members).map_err(|error| error.to_string())?;
    write_atomic(&dir.join(MEMBERS_FILE), &content)
}

pub fn cache_is_stale(modified: SystemTime, now: SystemTime) -> bool {
    now.duration_since(modified)
        .is_ok_and(|age| age > CACHE_MAX_AGE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir() -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "call-center-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn alice() -> Member {
        Member {
            user_id: String::from("U1"),
            name: String::from("Alice"),
        }
    }

    #[test]
    fn load_returns_empty_state_when_file_is_missing() {
        let dir = temp_dir();

        let state = load(&dir);

        assert!(state.favorites.is_empty());
        assert!(state.armed.is_empty());
    }

    #[test]
    fn corrupted_file_gives_empty_state_and_is_kept_as_backup() {
        let dir = temp_dir();
        fs::write(dir.join("state.json"), "{ pas du json").unwrap();

        let state = load(&dir);

        assert!(state.favorites.is_empty());
        assert_eq!(
            fs::read_to_string(dir.join("state.json.bak")).unwrap(),
            "{ pas du json"
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_dir();
        let state = State {
            favorites: vec![alice()],
            armed: HashSet::from([String::from("U1")]),
        };

        save(&dir, &state).unwrap();

        let loaded = load(&dir);
        assert_eq!(loaded.favorites, state.favorites);
        assert_eq!(loaded.armed, state.armed);
    }

    #[test]
    fn members_cache_round_trips_with_its_modification_time() {
        let dir = temp_dir();

        save_members(&dir, &[alice()]).unwrap();

        let (members, modified) = load_members(&dir).unwrap();
        assert_eq!(members, vec![alice()]);
        assert!(modified <= SystemTime::now());
    }

    #[test]
    fn missing_members_cache_is_none() {
        assert!(load_members(&temp_dir()).is_none());
    }

    #[test]
    fn cache_older_than_24h_is_stale() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 24 * 3600);

        assert!(cache_is_stale(
            now - Duration::from_secs(24 * 3600 + 1),
            now
        ));
        assert!(!cache_is_stale(now - Duration::from_secs(3600), now));
    }
}

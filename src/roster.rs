use crate::model::{Member, Reading};
use crate::store::{self, State};
use crate::watcher::Watcher;
use crate::window::Row;
use crate::{avatar, directory};
use std::path::PathBuf;
use std::time::SystemTime;

/// Favoris, annuaire et statuts : tout l'état de l'application, sans interface ni réseau.
/// Chaque changement qui doit survivre à un redémarrage est enregistré tout de suite.
pub struct Roster {
    favorites: Vec<Member>,
    directory: Vec<Member>,
    watcher: Watcher,
    /// Faux si le fichier d'état était illisible : on n'y écrit pas, pour ne rien écraser.
    persist: bool,
    directory_loaded_at: Option<SystemTime>,
    directory_loading: bool,
    config_dir: PathBuf,
    /// Dossier du cache des photos (`None` si HOME est inconnu : pastilles à initiales).
    avatars_dir: Option<PathBuf>,
}

impl Roster {
    pub fn new(
        state: State,
        persist: bool,
        directory: Vec<Member>,
        directory_loaded_at: Option<SystemTime>,
        config_dir: PathBuf,
        avatars_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            favorites: state.favorites,
            directory,
            watcher: Watcher::new(state.armed),
            persist,
            directory_loaded_at,
            directory_loading: false,
            config_dir,
            avatars_dir,
        }
    }

    fn save(&self) {
        if !self.persist {
            return;
        }
        let state = State {
            favorites: self.favorites.clone(),
            armed: self.watcher.armed_ids(),
        };
        if let Err(error) = store::save(&self.config_dir, &state) {
            eprintln!("Sauvegarde impossible : {error}");
        }
    }

    fn is_favorite(&self, user_id: &str) -> bool {
        self.favorites
            .iter()
            .any(|member| member.user_id == user_id)
    }

    pub fn favorites(&self) -> &[Member] {
        &self.favorites
    }

    pub fn favorite_ids(&self) -> Vec<String> {
        self.favorites
            .iter()
            .map(|member| member.user_id.clone())
            .collect()
    }

    /// Suggestions pour la saisie : l'annuaire, sans les collègues déjà en favoris.
    pub fn search(&self, query: &str, max: usize) -> Vec<Member> {
        directory::search(&self.directory, query, max + self.favorites.len())
            .into_iter()
            .filter(|member| !self.is_favorite(&member.user_id))
            .take(max)
            .cloned()
            .collect()
    }

    /// `false` si le collègue est déjà en favoris.
    pub fn add_favorite(&mut self, member: Member) -> bool {
        if self.is_favorite(&member.user_id) {
            return false;
        }
        self.favorites.push(member);
        self.save();
        true
    }

    pub fn remove_favorite(&mut self, user_id: &str) {
        self.favorites.retain(|member| member.user_id != user_id);
        self.watcher.forget(user_id);
        self.save();
    }

    /// Arme ou désarme la cloche (seulement sur un collègue en call) ; renvoie son nouvel état.
    pub fn toggle_bell(&mut self, user_id: &str) -> bool {
        let armed = self.watcher.toggle_bell(user_id);
        self.save();
        armed
    }

    pub fn armed_count(&self) -> usize {
        self.watcher.armed_ids().len()
    }

    /// Favoris dont la cloche est armée : les seuls revérifiés chaque minute.
    pub fn armed_favorite_ids(&self) -> Vec<String> {
        crate::schedule::users_to_poll(&self.favorite_ids(), &self.watcher.armed_ids())
    }

    #[cfg(test)]
    pub fn status(&self, user_id: &str) -> crate::model::Status {
        self.watcher.status(user_id)
    }

    /// Les statuts sans cloche ne sont plus présentés comme actuels (relus seulement à l'ouverture).
    pub fn expire_unarmed(&mut self) {
        self.watcher.expire_unarmed();
    }

    /// Applique des lectures et renvoie les collègues à prévenir (cloche armée, sortis de leur call).
    /// Un favori retiré pendant la lecture est ignoré.
    pub fn apply_readings(&mut self, readings: Vec<Reading>) -> Vec<Member> {
        let readings: Vec<Reading> = readings
            .into_iter()
            .filter(|reading| self.is_favorite(&reading.user_id))
            .collect();
        let to_notify = self
            .watcher
            .apply(&readings)
            .into_iter()
            .filter_map(|user_id| {
                self.favorites
                    .iter()
                    .find(|member| member.user_id == user_id)
                    .cloned()
            })
            .collect();
        self.save();
        to_notify
    }

    /// Photo en cache d'un collègue, si elle est déjà là.
    pub fn avatar_path(&self, member: &Member) -> Option<PathBuf> {
        let path = avatar::path_for(self.avatars_dir.as_deref()?, member)?;
        path.exists().then_some(path)
    }

    /// Lignes de la liste, par ordre alphabétique.
    pub fn rows(&self) -> Vec<Row> {
        let mut sorted = self.favorites.clone();
        directory::sort_alphabetically(&mut sorted);
        sorted
            .into_iter()
            .map(|member| Row {
                status: self.watcher.status(&member.user_id),
                armed: self.watcher.is_armed(&member.user_id),
                avatar: self.avatar_path(&member),
                member,
            })
            .collect()
    }

    /// `true` (et la marque en cours) si l'annuaire est vide, jamais chargé ou périmé, et qu'aucun
    /// rechargement n'est déjà en route.
    pub fn begin_directory_refresh_if_needed(&mut self, now: SystemTime) -> bool {
        if self.directory_loading {
            return false;
        }
        let needed =
            store::directory_needs_refresh(self.directory.len(), self.directory_loaded_at, now);
        self.directory_loading = needed;
        needed
    }

    pub fn directory_refresh_failed(&mut self) {
        self.directory_loading = false;
    }

    /// Nouvel annuaire : mis en cache, et les favoris sans photo (ou dont la photo a changé) sont complétés.
    pub fn apply_directory(&mut self, members: Vec<Member>, now: SystemTime) {
        self.directory_loading = false;
        self.directory_loaded_at = Some(now);
        if let Err(error) = store::save_members(&self.config_dir, &members) {
            eprintln!("Cache des membres impossible : {error}");
        }
        self.directory = members;
        if directory::refresh_favorites_from_directory(&mut self.favorites, &self.directory) {
            self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use std::collections::HashSet;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn temp_dir() -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "call-center-roster-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn member(user_id: &str, name: &str) -> Member {
        Member {
            user_id: user_id.to_string(),
            name: name.to_string(),
            avatar_url: None,
            team_id: None,
        }
    }

    fn reading(user_id: &str, status: Status) -> Reading {
        Reading {
            user_id: user_id.to_string(),
            status,
        }
    }

    fn roster_with(favorites: Vec<Member>, directory: Vec<Member>) -> (Roster, PathBuf) {
        let dir = temp_dir();
        let state = State {
            favorites,
            armed: HashSet::new(),
        };
        let roster = Roster::new(
            state,
            true,
            directory,
            None,
            dir.clone(),
            Some(dir.join("avatars")),
        );
        (roster, dir)
    }

    #[test]
    fn search_hides_colleagues_already_in_favorites_and_respects_the_limit() {
        let directory = vec![
            member("U1", "Anna"),
            member("U2", "Annie"),
            member("U3", "Anouk"),
        ];
        let (roster, _) = roster_with(vec![member("U1", "Anna")], directory);

        let found = roster.search("an", 1);

        assert_eq!(found, vec![member("U2", "Annie")]);
    }

    #[test]
    fn add_favorite_persists_it_and_refuses_duplicates() {
        let (mut roster, dir) = roster_with(vec![], vec![]);

        assert!(roster.add_favorite(member("U1", "Alice")));
        assert!(!roster.add_favorite(member("U1", "Alice")));

        assert_eq!(roster.favorite_ids(), vec![String::from("U1")]);
        assert_eq!(
            store::load(&dir).unwrap().favorites,
            vec![member("U1", "Alice")]
        );
    }

    #[test]
    fn nothing_is_written_when_persistence_is_disabled() {
        let dir = temp_dir();
        let mut roster = Roster::new(State::default(), false, vec![], None, dir.clone(), None);

        roster.add_favorite(member("U1", "Alice"));

        assert!(!dir.join("state.json").exists());
    }

    #[test]
    fn remove_favorite_forgets_its_status_and_bell() {
        let (mut roster, dir) = roster_with(vec![member("U1", "Alice")], vec![]);
        roster.apply_readings(vec![reading("U1", Status::InCall)]);
        roster.toggle_bell("U1");

        roster.remove_favorite("U1");

        assert!(roster.favorite_ids().is_empty());
        assert_eq!(roster.armed_count(), 0);
        assert!(store::load(&dir).unwrap().armed.is_empty());
    }

    #[test]
    fn bell_can_only_be_armed_on_a_colleague_in_call() {
        let (mut roster, _) = roster_with(vec![member("U1", "Alice")], vec![]);

        assert!(!roster.toggle_bell("U1"));
        roster.apply_readings(vec![reading("U1", Status::InCall)]);
        assert!(roster.toggle_bell("U1"));
        assert_eq!(roster.armed_count(), 1);
    }

    #[test]
    fn readings_return_the_colleagues_to_notify_when_they_leave_the_call() {
        let (mut roster, dir) = roster_with(vec![member("U1", "Alice")], vec![]);
        roster.apply_readings(vec![reading("U1", Status::InCall)]);
        roster.toggle_bell("U1");

        let to_notify = roster.apply_readings(vec![reading("U1", Status::Online)]);

        assert_eq!(to_notify, vec![member("U1", "Alice")]);
        assert_eq!(roster.armed_count(), 0);
        assert!(store::load(&dir).unwrap().armed.is_empty());
    }

    #[test]
    fn reading_of_a_removed_favorite_is_ignored() {
        let (mut roster, _) = roster_with(vec![member("U1", "Alice")], vec![]);
        roster.remove_favorite("U1");

        let to_notify = roster.apply_readings(vec![reading("U1", Status::InCall)]);

        assert!(to_notify.is_empty());
        assert_eq!(roster.status("U1"), Status::Unknown);
    }

    #[test]
    fn rows_are_sorted_alphabetically_with_status_and_bell() {
        let (mut roster, _) = roster_with(
            vec![
                member("U2", "zoé"),
                member("U1", "Éloïse"),
                member("U3", "Alice"),
            ],
            vec![],
        );
        roster.apply_readings(vec![
            reading("U1", Status::InCall),
            reading("U2", Status::Online),
        ]);
        roster.toggle_bell("U1");

        let rows = roster.rows();

        let names: Vec<&str> = rows.iter().map(|row| row.member.name.as_str()).collect();
        assert_eq!(names, vec!["Alice", "Éloïse", "zoé"]);
        assert_eq!(rows[0].status, Status::Unknown);
        assert_eq!(rows[1].status, Status::InCall);
        assert!(rows[1].armed);
        assert_eq!(rows[2].status, Status::Online);
        assert!(!rows[2].armed);
    }

    #[test]
    fn expiring_statuses_keeps_the_armed_ones_only() {
        let (mut roster, _) =
            roster_with(vec![member("U1", "Alice"), member("U2", "Bruno")], vec![]);
        roster.apply_readings(vec![
            reading("U1", Status::InCall),
            reading("U2", Status::InCall),
        ]);
        roster.toggle_bell("U1");

        roster.expire_unarmed();

        assert_eq!(roster.status("U1"), Status::InCall);
        assert_eq!(roster.status("U2"), Status::Unknown);
    }

    #[test]
    fn only_armed_favorites_are_polled_each_minute() {
        let (mut roster, _) =
            roster_with(vec![member("U1", "Alice"), member("U2", "Bruno")], vec![]);
        roster.apply_readings(vec![reading("U2", Status::InCall)]);
        roster.toggle_bell("U2");

        assert_eq!(roster.armed_favorite_ids(), vec![String::from("U2")]);
    }

    #[test]
    fn row_has_an_avatar_path_only_once_the_photo_is_in_cache() {
        let mut alice = member("U1", "Alice");
        alice.avatar_url = Some(String::from("https://avatars.example.com/a.jpg"));
        let (roster, dir) = roster_with(vec![alice.clone()], vec![]);
        assert_eq!(roster.rows()[0].avatar, None);

        let photo = avatar::path_for(&dir.join("avatars"), &alice).unwrap();
        fs::create_dir_all(photo.parent().unwrap()).unwrap();
        fs::write(&photo, b"photo").unwrap();

        assert_eq!(roster.rows()[0].avatar, Some(photo));
    }

    #[test]
    fn new_directory_completes_favorites_with_their_avatar() {
        let (mut roster, dir) = roster_with(vec![member("U1", "Alice")], vec![]);
        let mut known = member("U1", "Alice");
        known.avatar_url = Some(String::from("https://avatars.example.com/a.jpg"));
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

        roster.apply_directory(vec![known.clone()], now);

        assert_eq!(roster.favorites()[0].avatar_url, known.avatar_url);
        assert_eq!(
            store::load(&dir).unwrap().favorites[0].avatar_url,
            known.avatar_url
        );
        assert_eq!(roster.search("ali", 5), vec![]);
    }

    #[test]
    fn directory_refresh_is_needed_when_empty_stale_or_unloaded_and_never_twice_at_once() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 24 * 3600);
        let (mut roster, _) = roster_with(vec![], vec![]);

        assert!(roster.begin_directory_refresh_if_needed(now));
        assert!(!roster.begin_directory_refresh_if_needed(now));

        roster.apply_directory(vec![member("U1", "Alice")], now);
        assert!(!roster.begin_directory_refresh_if_needed(now));
        let later = now + Duration::from_secs(25 * 3600);
        assert!(roster.begin_directory_refresh_if_needed(later));
    }

    #[test]
    fn failed_directory_refresh_can_be_retried() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let (mut roster, _) = roster_with(vec![], vec![]);
        roster.begin_directory_refresh_if_needed(now);

        roster.directory_refresh_failed();

        assert!(roster.begin_directory_refresh_if_needed(now));
    }

    #[test]
    fn colleague_to_notify_carries_the_team_needed_for_the_conversation_link() {
        let mut alice = member("U1", "Alice");
        alice.team_id = Some(String::from("T1"));
        let (mut roster, _) = roster_with(vec![alice], vec![]);
        roster.apply_readings(vec![reading("U1", Status::InCall)]);
        roster.toggle_bell("U1");

        let to_notify = roster.apply_readings(vec![reading("U1", Status::Online)]);

        assert_eq!(to_notify[0].team_id.as_deref(), Some("T1"));
    }
}

use crate::model::{Reading, Status};
use std::collections::{HashMap, HashSet};

/// Dernier statut connu de chaque collègue et cloches armées.
///
/// Règle d'alerte unique : cloche armée ET nouvelle lecture valide « plus en huddle »
/// (en ligne ou hors ligne) → alerte, puis la cloche est désarmée. Une lecture `Unknown` ne change rien.
pub struct Watcher {
    statuses: HashMap<String, Status>,
    armed: HashSet<String>,
}

impl Watcher {
    pub fn new(armed: HashSet<String>) -> Self {
        Self {
            statuses: HashMap::new(),
            armed,
        }
    }

    /// Mémorise les lectures et renvoie les collègues dont la cloche se déclenche.
    pub fn apply(&mut self, readings: &[Reading]) -> Vec<String> {
        let mut fired = Vec::new();
        for reading in readings {
            self.statuses
                .insert(reading.user_id.clone(), reading.status);
            let left_the_call = matches!(reading.status, Status::Online | Status::Offline);
            if left_the_call && self.armed.remove(&reading.user_id) {
                fired.push(reading.user_id.clone());
            }
        }
        fired
    }

    pub fn status(&self, user_id: &str) -> Status {
        self.statuses
            .get(user_id)
            .copied()
            .unwrap_or(Status::Unknown)
    }

    /// Arme ou désarme la cloche ; sans effet si le collègue n'est pas en call. Renvoie l'état de la cloche.
    pub fn toggle_bell(&mut self, user_id: &str) -> bool {
        if self.armed.remove(user_id) {
            return false;
        }
        if self.status(user_id) == Status::InCall {
            self.armed.insert(user_id.to_string());
            return true;
        }
        false
    }

    /// Oublie un collègue retiré des favoris (statut et cloche).
    pub fn forget(&mut self, user_id: &str) {
        self.statuses.remove(user_id);
        self.armed.remove(user_id);
    }

    /// Les statuts sans cloche deviennent inconnus : ils ne sont plus relus automatiquement, donc
    /// on ne les présente pas comme actuels tant qu'une lecture fraîche n'est pas arrivée.
    pub fn expire_unarmed(&mut self) {
        let armed = &self.armed;
        self.statuses.retain(|user_id, _| armed.contains(user_id));
    }

    pub fn is_armed(&self, user_id: &str) -> bool {
        self.armed.contains(user_id)
    }

    pub fn armed_ids(&self) -> HashSet<String> {
        self.armed.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Reading, Status};
    use std::collections::HashSet;

    fn reading(user_id: &str, status: Status) -> Reading {
        Reading {
            user_id: user_id.to_string(),
            status,
        }
    }

    fn watcher_with_user_in_call(user_id: &str) -> Watcher {
        let mut watcher = Watcher::new(HashSet::new());
        watcher.apply(&[reading(user_id, Status::InCall)]);
        watcher
    }

    #[test]
    fn bell_fires_when_user_leaves_the_call() {
        let mut watcher = watcher_with_user_in_call("U1");
        watcher.toggle_bell("U1");

        let fired = watcher.apply(&[reading("U1", Status::Online)]);

        assert_eq!(fired, vec![String::from("U1")]);
    }

    #[test]
    fn bell_is_disarmed_after_firing() {
        let mut watcher = watcher_with_user_in_call("U1");
        watcher.toggle_bell("U1");
        watcher.apply(&[reading("U1", Status::Offline)]);

        let fired_again = watcher.apply(&[reading("U1", Status::Online)]);

        assert!(fired_again.is_empty());
        assert!(!watcher.is_armed("U1"));
    }

    #[test]
    fn unknown_reading_keeps_the_bell_armed_and_does_not_fire() {
        let mut watcher = watcher_with_user_in_call("U1");
        watcher.toggle_bell("U1");

        let fired = watcher.apply(&[reading("U1", Status::Unknown)]);

        assert!(fired.is_empty());
        assert!(watcher.is_armed("U1"));
        assert_eq!(watcher.status("U1"), Status::Unknown);
    }

    #[test]
    fn bell_cannot_be_armed_on_a_user_not_in_call() {
        let mut watcher = Watcher::new(HashSet::new());
        watcher.apply(&[reading("U1", Status::Online)]);

        let armed = watcher.toggle_bell("U1");

        assert!(!armed);
        assert!(!watcher.is_armed("U1"));
    }

    #[test]
    fn armed_bell_restored_after_restart_fires_on_first_valid_reading() {
        let mut watcher = Watcher::new(HashSet::from([String::from("U1")]));

        let fired = watcher.apply(&[reading("U1", Status::Offline)]);

        assert_eq!(fired, vec![String::from("U1")]);
    }

    #[test]
    fn still_in_call_keeps_the_bell_armed() {
        let mut watcher = watcher_with_user_in_call("U1");
        watcher.toggle_bell("U1");

        let fired = watcher.apply(&[reading("U1", Status::InCall)]);

        assert!(fired.is_empty());
        assert!(watcher.is_armed("U1"));
    }

    #[test]
    fn toggling_twice_disarms_the_bell() {
        let mut watcher = watcher_with_user_in_call("U1");

        assert!(watcher.toggle_bell("U1"));
        assert!(!watcher.toggle_bell("U1"));
    }

    #[test]
    fn forgotten_user_is_unknown_not_counted_and_not_armed() {
        let mut watcher = watcher_with_user_in_call("U1");
        watcher.toggle_bell("U1");

        watcher.forget("U1");

        assert_eq!(watcher.status("U1"), Status::Unknown);
        assert!(!watcher.is_armed("U1"));
    }

    #[test]
    fn expiring_statuses_keeps_only_the_armed_ones() {
        let mut watcher = Watcher::new(HashSet::new());
        watcher.apply(&[
            reading("U1", Status::InCall),
            reading("U2", Status::Online),
            reading("U3", Status::InCall),
        ]);
        watcher.toggle_bell("U1");

        watcher.expire_unarmed();

        assert_eq!(watcher.status("U1"), Status::InCall);
        assert_eq!(watcher.status("U2"), Status::Unknown);
        assert_eq!(watcher.status("U3"), Status::Unknown);
        assert!(watcher.is_armed("U1"));
    }
}

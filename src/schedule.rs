use std::collections::HashSet;

/// Une vérification complète (tous les favoris) toutes les 5 minutes, soit 1 tour sur 5.
pub const FULL_CHECK_EVERY_TICKS: u32 = 5;

/// Qui vérifier à ce tour (un tour par minute) : tout le monde tous les `FULL_CHECK_EVERY_TICKS` tours,
/// sinon seulement les favoris dont la cloche est armée (on attend leur sortie de call).
pub fn users_to_poll(tick: u32, favorites: &[String], armed: &HashSet<String>) -> Vec<String> {
    if tick.is_multiple_of(FULL_CHECK_EVERY_TICKS) {
        return favorites.to_vec();
    }
    favorites
        .iter()
        .filter(|user_id| armed.contains(*user_id))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn armed(values: &[&str]) -> HashSet<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn first_tick_checks_every_favorite() {
        let favorites = ids(&["U1", "U2", "U3"]);

        assert_eq!(users_to_poll(0, &favorites, &armed(&["U2"])), favorites);
    }

    #[test]
    fn intermediate_ticks_check_only_armed_favorites() {
        let favorites = ids(&["U1", "U2", "U3"]);

        for tick in 1..FULL_CHECK_EVERY_TICKS {
            assert_eq!(
                users_to_poll(tick, &favorites, &armed(&["U3", "U1"])),
                ids(&["U1", "U3"]),
                "tick {tick}"
            );
        }
    }

    #[test]
    fn every_fifth_tick_checks_everyone_again() {
        let favorites = ids(&["U1", "U2"]);

        assert_eq!(
            users_to_poll(FULL_CHECK_EVERY_TICKS, &favorites, &armed(&[])),
            favorites
        );
        assert_eq!(
            users_to_poll(2 * FULL_CHECK_EVERY_TICKS, &favorites, &armed(&[])),
            favorites
        );
    }

    #[test]
    fn intermediate_tick_without_armed_bell_checks_nobody() {
        assert!(users_to_poll(1, &ids(&["U1"]), &armed(&[])).is_empty());
    }

    #[test]
    fn armed_id_that_is_no_longer_a_favorite_is_ignored() {
        assert!(users_to_poll(1, &ids(&["U1"]), &armed(&["U9"])).is_empty());
    }
}

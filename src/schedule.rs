use std::collections::HashSet;

/// Qui vérifier à chaque tour de minuterie (une fois par minute) : seulement les favoris dont la cloche est
/// armée, puisqu'on attend leur sortie de call. Les autres ne sont relus qu'à l'ouverture de la fenêtre.
pub fn users_to_poll(favorites: &[String], armed: &HashSet<String>) -> Vec<String> {
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
    fn only_armed_favorites_are_polled_in_favorite_order() {
        let favorites = ids(&["U1", "U2", "U3"]);

        assert_eq!(
            users_to_poll(&favorites, &armed(&["U3", "U1"])),
            ids(&["U1", "U3"])
        );
    }

    #[test]
    fn nobody_is_polled_without_an_armed_bell() {
        assert!(users_to_poll(&ids(&["U1", "U2"]), &armed(&[])).is_empty());
    }

    #[test]
    fn armed_id_that_is_no_longer_a_favorite_is_ignored() {
        assert!(users_to_poll(&ids(&["U1"]), &armed(&["U9"])).is_empty());
    }
}

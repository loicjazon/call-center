use crate::model::Member;

/// Minuscules sans accents (lettres latines courantes) pour comparer des noms.
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|letter| match letter {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            other => other,
        })
        .collect()
}

/// Complète les favoris (enregistrés avant les avatars et le lien de conversation, ou dont la photo a
/// changé) avec l'annuaire. Renvoie `true` si au moins un favori a changé.
pub fn refresh_favorites_from_directory(favorites: &mut [Member], directory: &[Member]) -> bool {
    let mut changed = false;
    for favorite in favorites.iter_mut() {
        let Some(known) = directory
            .iter()
            .find(|member| member.user_id == favorite.user_id)
        else {
            continue;
        };
        if known.avatar_url.is_some() && known.avatar_url != favorite.avatar_url {
            favorite.avatar_url = known.avatar_url.clone();
            changed = true;
        }
        if known.team_id.is_some() && known.team_id != favorite.team_id {
            favorite.team_id = known.team_id.clone();
            changed = true;
        }
    }
    changed
}

/// Tri alphabétique, casse et accents ignorés.
pub fn sort_alphabetically(members: &mut [Member]) {
    members.sort_by_cached_key(|member| fold(&member.name));
}

/// Membres dont le nom contient la requête (casse et accents ignorés) ; les noms qui commencent
/// par la requête, ou dont un mot la commence, passent avant. Requête vide : aucun résultat.
pub fn search<'a>(members: &'a [Member], query: &str, limit: usize) -> Vec<&'a Member> {
    let needle = fold(query.trim());
    if needle.is_empty() {
        return Vec::new();
    }
    let mut ranked: Vec<(usize, &Member)> = members
        .iter()
        .filter_map(|member| {
            let name = fold(&member.name);
            let rank = if name.starts_with(&needle) {
                0
            } else if name.split([' ', '-']).any(|word| word.starts_with(&needle)) {
                1
            } else if name.contains(&needle) {
                2
            } else {
                return None;
            };
            Some((rank, member))
        })
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked
        .into_iter()
        .map(|(_, member)| member)
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Member;

    fn member(user_id: &str, name: &str) -> Member {
        Member {
            user_id: user_id.to_string(),
            name: name.to_string(),
            avatar_url: None,
            team_id: None,
        }
    }

    #[test]
    fn search_ignores_case_and_accents() {
        let members = vec![member("U1", "Éloïse Exemple"), member("U2", "Bruno")];

        let found = search(&members, "eLOISE", 10);

        assert_eq!(found, vec![&members[0]]);
    }

    #[test]
    fn search_finds_unaccented_name_with_accented_query() {
        let members = vec![member("U1", "Eloise")];

        let found = search(&members, "Éloï", 10);

        assert_eq!(found, vec![&members[0]]);
    }

    #[test]
    fn prefix_matches_come_first() {
        let members = vec![member("U1", "Jean-Marc"), member("U2", "Marc Exemple")];

        let found = search(&members, "marc", 10);

        assert_eq!(found, vec![&members[1], &members[0]]);
    }

    #[test]
    fn empty_query_returns_nothing() {
        let members = vec![member("U1", "Alice")];

        assert!(search(&members, "  ", 10).is_empty());
    }

    #[test]
    fn search_respects_the_limit() {
        let members = vec![
            member("U1", "Ana"),
            member("U2", "Anaïs"),
            member("U3", "Anne"),
        ];

        assert_eq!(search(&members, "an", 2).len(), 2);
    }

    #[test]
    fn members_are_sorted_alphabetically_ignoring_case_and_accents() {
        let mut members = vec![
            member("U1", "zoé"),
            member("U2", "Éloïse"),
            member("U3", "bruno"),
            member("U4", "Alice"),
            member("U5", "Fanny"),
        ];

        sort_alphabetically(&mut members);

        let names: Vec<&str> = members.iter().map(|member| member.name.as_str()).collect();
        assert_eq!(names, vec!["Alice", "bruno", "Éloïse", "Fanny", "zoé"]);
    }

    #[test]
    fn favorites_get_their_avatar_and_team_from_the_directory() {
        let mut favorites = vec![member("U1", "Alice"), member("U2", "Bruno")];
        let mut known = member("U1", "Alice");
        known.avatar_url = Some(String::from("https://avatars.example.com/a.jpg"));
        known.team_id = Some(String::from("T1"));

        let changed = refresh_favorites_from_directory(&mut favorites, &[known]);

        assert!(changed);
        assert_eq!(favorites[0].team_id.as_deref(), Some("T1"));
        assert_eq!(favorites[1].team_id, None);
        assert_eq!(
            favorites[0].avatar_url.as_deref(),
            Some("https://avatars.example.com/a.jpg")
        );
        assert_eq!(favorites[1].avatar_url, None);
    }

    #[test]
    fn refreshing_avatars_reports_no_change_when_nothing_differs() {
        let mut favorites = vec![member("U1", "Alice")];

        let changed = refresh_favorites_from_directory(&mut favorites, &[member("U1", "Alice")]);

        assert!(!changed);
    }
}

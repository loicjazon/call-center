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
}

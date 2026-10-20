use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::{env, fs};

/// Nom de l'icône : un fichier `call-center.svg` écrit au lancement dans `icon_dir()`.
const ICON_NAME: &str = "call-center";
/// Logo embarqué dans le binaire (distribué seul, sans fichier annexe).
const ICON_SVG: &str = include_str!("../assets/logo.svg");
/// Icône du thème, utilisée si le logo ne peut pas être écrit sur le disque.
const FALLBACK_ICON: &str = "call-start-symbolic";

/// Texte à côté de l'icône. Les statuts inconnus ne sont jamais comptés comme « libres ».
pub fn label(in_call: usize, unknown: usize) -> String {
    match (in_call, unknown) {
        (0, 0) => String::from("libre"),
        (count, 0) => format!("{count} en call"),
        (0, unknown) => format!("{unknown} ?"),
        (count, unknown) => format!("{count} en call · {unknown} ?"),
    }
}

/// Ligne d'information du menu.
pub fn summary(in_call: usize, unknown: usize) -> String {
    let calls = match in_call {
        0 => String::from("Personne en call"),
        1 => String::from("1 collègue en call"),
        count => format!("{count} collègues en call"),
    };
    let unknowns = match unknown {
        0 => return calls,
        1 => String::from("1 statut inconnu"),
        count => format!("{count} statuts inconnus"),
    };
    if in_call == 0 {
        unknowns
    } else {
        format!("{calls} · {unknowns}")
    }
}

/// Dossier où est écrit le logo (`$XDG_DATA_HOME/call-center/icons`, par défaut `~/.local/share/...`).
fn icon_dir() -> Option<PathBuf> {
    let data_home = match env::var_os("XDG_DATA_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from(env::var_os("HOME")?).join(".local/share"),
    };
    Some(data_home.join("call-center/icons"))
}

/// Écrit le logo sur le disque (la barre le charge par nom) ; renvoie son dossier, ou `None` en cas d'échec.
fn write_icon() -> Option<PathBuf> {
    let dir = icon_dir()?;
    fs::create_dir_all(&dir).ok()?;
    let file = dir.join(format!("{ICON_NAME}.svg"));
    if fs::read_to_string(&file).ok().as_deref() != Some(ICON_SVG) {
        fs::write(&file, ICON_SVG).ok()?;
    }
    Some(dir)
}

/// Indicateur dans la barre supérieure du bureau (StatusNotifier / AppIndicator).
pub struct Tray {
    indicator: AppIndicator,
    summary_line: gtk::MenuItem,
}

impl Tray {
    /// `None` si la bibliothèque appindicator n'est pas installée (l'app continue sans indicateur).
    /// `on_open` est appelé par l'entrée « Ouvrir la liste… ».
    pub fn new(on_open: impl Fn() + 'static) -> Option<Self> {
        let icon_dir = write_icon();
        let created = catch_unwind(AssertUnwindSafe(|| match &icon_dir {
            Some(dir) => AppIndicator::with_path("call-center", ICON_NAME, &dir.to_string_lossy()),
            None => AppIndicator::new("call-center", FALLBACK_ICON),
        }));
        let mut indicator = match created {
            Ok(indicator) => indicator,
            Err(_) => {
                eprintln!(
                    "Indicateur de la barre indisponible : installez libayatana-appindicator3-1 \
                     (sudo apt install libayatana-appindicator3-1)"
                );
                return None;
            }
        };

        let summary_line = gtk::MenuItem::with_label("");
        // Ligne d'information : grisée, non cliquable.
        summary_line.set_sensitive(false);
        let open = gtk::MenuItem::with_label("Ouvrir la liste…");
        open.connect_activate(move |_| on_open());
        let quit = gtk::MenuItem::with_label("Quitter call-center");
        quit.connect_activate(|_| gtk::main_quit());

        let mut menu = gtk::Menu::new();
        menu.append(&summary_line);
        menu.append(&open);
        menu.append(&gtk::SeparatorMenuItem::new());
        menu.append(&quit);
        menu.show_all();

        // Texte provisoire : la barre ignore le tout premier texte posé, elle ne réagit qu'à un changement.
        indicator.set_label("…", "");
        indicator.set_status(AppIndicatorStatus::Active);
        indicator.set_menu(&mut menu);

        Some(Self {
            indicator,
            summary_line,
        })
    }

    /// Met à jour le texte à côté de l'icône et la ligne du menu.
    pub fn show(&mut self, in_call: usize, unknown: usize) {
        self.indicator.set_label(&label(in_call, unknown), "");
        self.summary_line.set_label(&summary(in_call, unknown));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_when_nobody_is_in_call() {
        assert_eq!(label(0, 0), "libre");
    }

    #[test]
    fn label_counts_people_in_call() {
        assert_eq!(label(2, 0), "2 en call");
    }

    #[test]
    fn label_does_not_say_free_when_every_status_is_unknown() {
        assert_eq!(label(0, 3), "3 ?");
    }

    #[test]
    fn label_shows_both_calls_and_unknowns() {
        assert_eq!(label(1, 2), "1 en call · 2 ?");
    }

    #[test]
    fn summary_is_singular_for_one_person() {
        assert_eq!(summary(1, 0), "1 collègue en call");
        assert_eq!(summary(0, 0), "Personne en call");
        assert_eq!(summary(3, 0), "3 collègues en call");
    }

    #[test]
    fn summary_mentions_unknown_statuses() {
        assert_eq!(summary(0, 1), "1 statut inconnu");
        assert_eq!(summary(0, 2), "2 statuts inconnus");
        assert_eq!(summary(2, 1), "2 collègues en call · 1 statut inconnu");
    }
}

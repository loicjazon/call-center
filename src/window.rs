use crate::model::{Member, Status};
use gtk::gdk::prelude::*;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::glib;
use gtk::prelude::*;
use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Côté de l'avatar affiché, en pixels logiques.
const AVATAR_SIZE: i32 = 32;

/// Une ligne de la liste : collègue, statut et cloche.
pub struct Row {
    pub member: Member,
    pub status: Status,
    pub armed: bool,
    /// Photo en cache, si elle est déjà téléchargée.
    pub avatar: Option<PathBuf>,
}

/// Suggestions de collègues pour la saisie en cours.
pub type SearchFn = Box<dyn Fn(&str) -> Vec<Member>>;

/// Ce que la fenêtre demande à l'application ; la fenêtre ne connaît ni Slack ni le disque.
pub struct Callbacks {
    /// Suggestions pour la saisie en cours.
    pub search: SearchFn,
    pub add: Box<dyn Fn(Member)>,
    pub remove: Box<dyn Fn(&str)>,
    /// Bascule la cloche ; renvoie son nouvel état.
    pub toggle_bell: Box<dyn Fn(&str) -> bool>,
    /// Lignes à afficher, dans l'ordre.
    pub rows: Box<dyn Fn() -> Vec<Row>>,
}

/// Fenêtre de la liste (maquette A) : recherche en haut, collègues dessous.
pub struct ListWindow {
    window: gtk::Window,
    list: gtk::ListBox,
    callbacks: Rc<Callbacks>,
}

/// Avatar rond ; sans photo (ou photo illisible) : pastille grise avec l'initiale.
fn avatar_widget(name: &str, path: Option<&Path>) -> gtk::DrawingArea {
    // Chargée au double de la taille affichée pour rester nette sur les écrans HiDPI.
    let pixbuf = path.and_then(|path| {
        Pixbuf::from_file_at_scale(path, AVATAR_SIZE * 2, AVATAR_SIZE * 2, true).ok()
    });
    let initial = name
        .chars()
        .next()
        .map(|letter| letter.to_uppercase().to_string())
        .unwrap_or_default();

    let area = gtk::DrawingArea::new();
    area.set_size_request(AVATAR_SIZE, AVATAR_SIZE);
    area.set_valign(gtk::Align::Center);
    area.connect_draw(move |_, context| {
        let size = f64::from(AVATAR_SIZE);
        context.arc(size / 2.0, size / 2.0, size / 2.0, 0.0, 2.0 * PI);
        context.clip();
        match &pixbuf {
            Some(pixbuf) => {
                context.scale(0.5, 0.5);
                context.set_source_pixbuf(pixbuf, 0.0, 0.0);
                let _ = context.paint();
            }
            None => {
                context.set_source_rgb(0.55, 0.60, 0.70);
                let _ = context.paint();
                context.set_source_rgb(1.0, 1.0, 1.0);
                context.set_font_size(16.0);
                if let Ok(extents) = context.text_extents(&initial) {
                    context.move_to(
                        (size - extents.width()) / 2.0 - extents.x_bearing(),
                        (size - extents.height()) / 2.0 - extents.y_bearing(),
                    );
                    let _ = context.show_text(&initial);
                }
            }
        }
        glib::Propagation::Proceed
    });
    area
}

fn row_widget(row: &Row, list: &gtk::ListBox, callbacks: &Rc<Callbacks>) -> gtk::ListBoxRow {
    let bell = if row.armed { " 🔔" } else { "" };
    let text = format!(
        "{} {} — {}{bell}",
        row.status.emoji(),
        row.member.name,
        row.status.label()
    );
    let label = gtk::Label::new(Some(&text));
    label.set_xalign(0.0);
    label.set_hexpand(true);

    let remove = gtk::Button::with_label("✕");
    remove.set_relief(gtk::ReliefStyle::None);
    remove.set_tooltip_text(Some("Retirer des favoris"));
    let user_id = row.member.user_id.clone();
    let (list_ref, callbacks_ref) = (list.clone(), Rc::clone(callbacks));
    remove.connect_clicked(move |_| {
        (callbacks_ref.remove)(&user_id);
        populate(&list_ref, &callbacks_ref);
    });

    let line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    line.set_margin_start(8);
    line.set_margin_end(4);
    line.set_margin_top(4);
    line.set_margin_bottom(4);
    line.pack_start(
        &avatar_widget(&row.member.name, row.avatar.as_deref()),
        false,
        false,
        0,
    );
    line.pack_start(&label, true, true, 0);
    line.pack_end(&remove, false, false, 0);

    let list_row = gtk::ListBoxRow::new();
    list_row.add(&line);
    // L'identifiant retrouve le collègue au clic sur la ligne.
    list_row.set_widget_name(&row.member.user_id);
    list_row
}

/// Reconstruit la liste à partir des lignes fournies par l'application.
fn populate(list: &gtk::ListBox, callbacks: &Rc<Callbacks>) {
    for child in list.children() {
        list.remove(&child);
    }
    for row in (callbacks.rows)() {
        list.add(&row_widget(&row, list, callbacks));
    }
    list.show_all();
}

impl ListWindow {
    pub fn new(callbacks: Callbacks) -> Self {
        let callbacks = Rc::new(callbacks);

        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Call Center");
        window.set_default_size(380, 420);
        window.set_position(gtk::WindowPosition::Center);
        // Fermer cache la fenêtre : l'application continue dans la barre.
        window.connect_delete_event(|window, _| {
            window.hide();
            glib::Propagation::Stop
        });

        let suggestions =
            gtk::ListStore::new(&[glib::Type::STRING, glib::Type::STRING, glib::Type::STRING]);
        let completion = gtk::EntryCompletion::new();
        completion.set_model(Some(&suggestions));
        completion.set_text_column(0);
        completion.set_minimum_key_length(1);
        // Le filtrage (casse, accents) est déjà fait par `search` : on affiche tout le modèle.
        completion.set_match_func(|_, _, _| true);

        let entry = gtk::SearchEntry::new();
        entry.set_placeholder_text(Some("Ajouter un collègue…"));
        entry.set_completion(Some(&completion));

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);

        let (suggestions_ref, callbacks_ref) = (suggestions.clone(), Rc::clone(&callbacks));
        entry.connect_changed(move |entry| {
            suggestions_ref.clear();
            for member in (callbacks_ref.search)(&entry.text()) {
                let avatar_url = member.avatar_url.clone().unwrap_or_default();
                suggestions_ref.insert_with_values(
                    None,
                    &[(0, &member.name), (1, &member.user_id), (2, &avatar_url)],
                );
            }
        });

        let (list_ref, callbacks_ref) = (list.clone(), Rc::clone(&callbacks));
        let entry_ref = entry.clone();
        completion.connect_match_selected(move |_, model, iter| {
            let name = model.value(iter, 0).get::<String>().unwrap_or_default();
            let user_id = model.value(iter, 1).get::<String>().unwrap_or_default();
            let avatar_url = model
                .value(iter, 2)
                .get::<String>()
                .ok()
                .filter(|url| !url.is_empty());
            if !user_id.is_empty() {
                (callbacks_ref.add)(Member {
                    user_id,
                    name,
                    avatar_url,
                });
                populate(&list_ref, &callbacks_ref);
            }
            entry_ref.set_text("");
            glib::Propagation::Stop
        });

        let (list_ref, callbacks_ref) = (list.clone(), Rc::clone(&callbacks));
        list.connect_row_activated(move |_, row| {
            (callbacks_ref.toggle_bell)(&row.widget_name());
            populate(&list_ref, &callbacks_ref);
        });

        let scroller = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scroller.set_vexpand(true);
        scroller.add(&list);

        let hint = gtk::Label::new(Some(
            "Clic sur un collègue en call : activer ou retirer la cloche 🔔",
        ));
        hint.set_xalign(0.0);
        hint.set_margin_start(8);
        hint.set_margin_bottom(6);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.set_margin_top(8);
        content.set_margin_start(8);
        content.set_margin_end(8);
        content.pack_start(&entry, false, false, 0);
        content.pack_start(&scroller, true, true, 0);
        content.pack_start(&hint, false, false, 0);
        window.add(&content);

        Self {
            window,
            list,
            callbacks,
        }
    }

    /// Affiche la fenêtre au premier plan, liste à jour.
    pub fn present(&self) {
        self.refresh();
        self.window.show_all();
        self.window.present();
    }

    /// Relit les lignes auprès de l'application et redessine la liste.
    pub fn refresh(&self) {
        populate(&self.list, &self.callbacks);
    }
}

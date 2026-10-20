mod avatar;
mod client;
mod config;
mod directory;
mod gate;
mod model;
mod notify;
mod probe;
mod roster;
mod schedule;
mod slack;
mod store;
#[cfg(test)]
mod test_support;
mod tray;
mod watcher;
mod window;

use client::{SlackClient, spawn_poll};
use gate::PollGate;
use model::{Member, Reading};
use roster::Roster;
use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::SystemTime;
use store::State;
use tray::Tray;
use window::{Callbacks, ListWindow};

/// Nombre maximal de suggestions affichées sous le champ de recherche.
const MAX_SUGGESTIONS: usize = 8;

/// Partie « branchements » de l'application : le `Roster` (état, testé) et ce qui lance les lectures
/// en arrière-plan. Partagée par les rappels de l'interface (un seul thread : la boucle GTK).
struct App {
    roster: Roster,
    /// Une seule vérification en vol à la fois (voir `PollGate`).
    gate: RefCell<PollGate>,
    client: Arc<SlackClient>,
    readings_tx: Sender<Vec<Reading>>,
    members_tx: Sender<Result<Vec<Member>, String>>,
    avatars_dir: Option<std::path::PathBuf>,
    avatars_tx: Sender<()>,
}

impl App {
    /// Demande une vérification : lancée tout de suite, ou regroupée avec la suivante si une est en cours.
    fn poll(&self) {
        if self.gate.borrow_mut().request() {
            self.start_poll();
        }
    }

    /// Un tour de la minuterie (chaque minute) : revérifie seulement les cloches armées.
    /// Sauté si une vérification est déjà en cours.
    fn poll_tick(&self) {
        let user_ids = self.roster.armed_favorite_ids();
        if !user_ids.is_empty() && self.gate.borrow_mut().try_start() {
            self.start_poll_for(user_ids);
        }
    }

    /// Lance une vérification de tous les favoris (résultat lu par la boucle GTK).
    /// Une demande regroupée pendant une vérification en cours est rejouée ainsi, pour tout le monde.
    fn start_poll(&self) {
        self.start_poll_for(self.roster.favorite_ids());
    }

    fn start_poll_for(&self, user_ids: Vec<String>) {
        spawn_poll(Arc::clone(&self.client), user_ids, self.readings_tx.clone());
    }

    /// Recharge l'annuaire en arrière-plan s'il est vide ou périmé (au démarrage et à chaque ouverture).
    fn refresh_directory_if_needed(&mut self) {
        if self
            .roster
            .begin_directory_refresh_if_needed(SystemTime::now())
        {
            let client = Arc::clone(&self.client);
            let sender = self.members_tx.clone();
            std::thread::spawn(move || {
                let _ = sender.send(client.list_members());
            });
        }
    }

    /// Télécharge en arrière-plan les photos des favoris qui ne sont pas encore en cache.
    fn fetch_avatars(&self) {
        if let Some(dir) = &self.avatars_dir {
            avatar::spawn_downloads(
                dir.clone(),
                self.roster.favorites().to_vec(),
                self.avatars_tx.clone(),
            );
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let Some(config_dir) = config::config_dir() else {
        eprintln!("Dossier de configuration introuvable (HOME non défini)");
        return;
    };
    let token = match config::read_token(&config_dir) {
        Ok(token) => token,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    if args.get(1).map(String::as_str) == Some("probe") {
        let argument = args.get(2).map(String::as_str).unwrap_or("members");
        if let Err(error) = probe::run(&token, argument) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }

    // GTK doit être initialisé avant de créer une fenêtre (échoue s'il n'y a pas d'écran).
    if let Err(error) = gtk::init() {
        eprintln!("Impossible d'initialiser GTK : {error}");
        return;
    }

    let (state, persist) = match store::load(&config_dir) {
        Ok(state) => (state, true),
        Err(error) => {
            eprintln!("{error} : favoris non chargés, aucune sauvegarde pendant cette session");
            (State::default(), false)
        }
    };
    let (readings_tx, readings_rx): (Sender<Vec<Reading>>, Receiver<Vec<Reading>>) =
        mpsc::channel();
    let (members_tx, members_rx) = mpsc::channel::<Result<Vec<Member>, String>>();
    let (avatars_tx, avatars_rx) = mpsc::channel::<()>();

    let cached = store::load_members(&config_dir);
    let directory_loaded_at = cached.as_ref().map(|(_, modified)| *modified);
    let avatars_dir = avatar::cache_dir();
    let app = Rc::new(RefCell::new(App {
        roster: Roster::new(
            state,
            persist,
            cached.map(|(members, _)| members).unwrap_or_default(),
            directory_loaded_at,
            config_dir,
            avatars_dir.clone(),
        ),
        gate: RefCell::new(PollGate::default()),
        client: Arc::new(SlackClient::new(token)),
        readings_tx,
        members_tx,
        avatars_dir,
        avatars_tx,
    }));
    app.borrow().fetch_avatars();
    app.borrow_mut().refresh_directory_if_needed();

    // L'indicateur ne montre que les cloches armées ; créé après la fenêtre (il l'ouvre), d'où la cellule vide.
    let tray: Rc<RefCell<Option<Tray>>> = Rc::new(RefCell::new(None));
    let refresh_tray: Rc<dyn Fn()> = Rc::new({
        let (app, tray) = (Rc::clone(&app), Rc::clone(&tray));
        move || {
            let armed = app.borrow().roster.armed_count();
            if let Some(tray) = tray.borrow_mut().as_mut() {
                tray.show(armed);
            }
        }
    });

    let window = Rc::new(ListWindow::new(Callbacks {
        search: Box::new({
            let app = Rc::clone(&app);
            move |query| app.borrow().roster.search(query, MAX_SUGGESTIONS)
        }),
        add: Box::new({
            let app = Rc::clone(&app);
            move |member| {
                let mut app = app.borrow_mut();
                if app.roster.add_favorite(member) {
                    app.poll();
                    app.fetch_avatars();
                }
            }
        }),
        remove: Box::new({
            let (app, refresh_tray) = (Rc::clone(&app), Rc::clone(&refresh_tray));
            move |user_id| {
                app.borrow_mut().roster.remove_favorite(user_id);
                refresh_tray();
            }
        }),
        toggle_bell: Box::new({
            let (app, refresh_tray) = (Rc::clone(&app), Rc::clone(&refresh_tray));
            move |user_id| {
                let armed = app.borrow_mut().roster.toggle_bell(user_id);
                refresh_tray();
                armed
            }
        }),
        rows: Box::new({
            let app = Rc::clone(&app);
            move || app.borrow().roster.rows()
        }),
    }));

    // Sans indicateur (bibliothèque absente), l'application continue sans icône mais les alertes fonctionnent.
    *tray.borrow_mut() = Tray::new({
        let (app, window) = (Rc::clone(&app), Rc::clone(&window));
        move || {
            // Les statuts sans cloche ne sont relus qu'ici : en attendant la lecture fraîche, ils
            // s'affichent « inconnu » plutôt qu'avec une valeur périmée.
            app.borrow_mut().roster.expire_unarmed();
            window.present();
            app.borrow_mut().refresh_directory_if_needed();
            app.borrow().poll();
        }
    });
    refresh_tray();

    // Les cloches (persistées) sont revérifiées peu après le démarrage, puis chaque minute.
    let first_app = Rc::clone(&app);
    gtk::glib::timeout_add_seconds_local_once(config::FIRST_CHECK_SECONDS, move || {
        first_app.borrow().poll_tick();
    });
    let periodic_app = Rc::clone(&app);
    gtk::glib::timeout_add_seconds_local(config::CHECK_INTERVAL_SECONDS, move || {
        periodic_app.borrow().poll_tick();
        gtk::glib::ControlFlow::Continue
    });

    // Les threads de lecture répondent par des canaux ; la boucle GTK les vide chaque seconde.
    gtk::glib::timeout_add_seconds_local(1, move || {
        while let Ok(result) = members_rx.try_recv() {
            let mut app = app.borrow_mut();
            match result {
                Ok(members) => {
                    app.roster.apply_directory(members, SystemTime::now());
                    app.fetch_avatars();
                }
                Err(error) => {
                    app.roster.directory_refresh_failed();
                    eprintln!("Annuaire Slack indisponible : {error}");
                }
            }
        }
        let mut avatars_arrived = false;
        while avatars_rx.try_recv().is_ok() {
            avatars_arrived = true;
        }
        if avatars_arrived {
            window.refresh();
        }
        while let Ok(readings) = readings_rx.try_recv() {
            let mut app = app.borrow_mut();
            for member in app.roster.apply_readings(readings) {
                let logo = tray::logo_path();
                notify::send(
                    &member.name,
                    logo.as_deref(),
                    app.roster.avatar_path(&member).as_deref(),
                );
            }
            // Une demande arrivée pendant la vérification est relancée maintenant, une seule fois.
            if app.gate.borrow_mut().finished() {
                app.start_poll();
            }
            drop(app);
            refresh_tray();
            window.refresh();
        }
        gtk::glib::ControlFlow::Continue
    });
    gtk::main();
}

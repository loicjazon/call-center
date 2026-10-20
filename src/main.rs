mod client;
mod config;
mod directory;
mod model;
mod notify;
mod probe;
mod slack;
mod store;
mod tray;
mod watcher;
mod window;

use client::{SlackClient, spawn_poll};
use model::{Member, Reading};
use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::SystemTime;
use store::State;
use tray::Tray;
use watcher::Watcher;
use window::{Callbacks, ListWindow, Row};

/// Nombre maximal de suggestions affichées sous le champ de recherche.
const MAX_SUGGESTIONS: usize = 8;

/// État partagé par les rappels de l'interface (un seul thread : la boucle GTK).
struct App {
    config_dir: std::path::PathBuf,
    favorites: Vec<Member>,
    directory: Vec<Member>,
    watcher: Watcher,
    client: Arc<SlackClient>,
    readings_tx: Sender<Vec<Reading>>,
    members_tx: Sender<Result<Vec<Member>, String>>,
}

impl App {
    fn save(&self) {
        let state = State {
            favorites: self.favorites.clone(),
            armed: self.watcher.armed_ids(),
        };
        if let Err(error) = store::save(&self.config_dir, &state) {
            eprintln!("Sauvegarde impossible : {error}");
        }
    }

    /// Lance une vérification de tous les favoris (résultat lu par la boucle GTK).
    fn poll(&self) {
        let user_ids = self
            .favorites
            .iter()
            .map(|member| member.user_id.clone())
            .collect();
        spawn_poll(Arc::clone(&self.client), user_ids, self.readings_tx.clone());
    }

    /// Recharge l'annuaire en arrière-plan.
    fn refresh_directory(&self) {
        let client = Arc::clone(&self.client);
        let sender = self.members_tx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(client.list_members());
        });
    }

    fn rows(&self) -> Vec<Row> {
        self.favorites
            .iter()
            .map(|member| Row {
                member: member.clone(),
                status: self.watcher.status(&member.user_id),
                armed: self.watcher.is_armed(&member.user_id),
            })
            .collect()
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

    let state = store::load(&config_dir);
    let (readings_tx, readings_rx): (Sender<Vec<Reading>>, Receiver<Vec<Reading>>) =
        mpsc::channel();
    let (members_tx, members_rx) = mpsc::channel::<Result<Vec<Member>, String>>();

    let cached = store::load_members(&config_dir);
    let cache_is_fresh = cached
        .as_ref()
        .is_some_and(|(_, modified)| !store::cache_is_stale(*modified, SystemTime::now()));
    let app = Rc::new(RefCell::new(App {
        config_dir: config_dir.clone(),
        favorites: state.favorites,
        directory: cached.map(|(members, _)| members).unwrap_or_default(),
        watcher: Watcher::new(state.armed),
        client: Arc::new(SlackClient::new(token)),
        readings_tx,
        members_tx,
    }));
    if !cache_is_fresh {
        app.borrow().refresh_directory();
    }

    let window = Rc::new(ListWindow::new(Callbacks {
        search: Box::new({
            let app = Rc::clone(&app);
            move |query| {
                let app = app.borrow();
                directory::search(&app.directory, query, MAX_SUGGESTIONS + app.favorites.len())
                    .into_iter()
                    .filter(|member| {
                        !app.favorites
                            .iter()
                            .any(|fav| fav.user_id == member.user_id)
                    })
                    .take(MAX_SUGGESTIONS)
                    .cloned()
                    .collect()
            }
        }),
        add: Box::new({
            let app = Rc::clone(&app);
            move |member| {
                let mut app = app.borrow_mut();
                if !app
                    .favorites
                    .iter()
                    .any(|fav| fav.user_id == member.user_id)
                {
                    app.favorites.push(member);
                    app.save();
                    app.poll();
                }
            }
        }),
        remove: Box::new({
            let app = Rc::clone(&app);
            move |user_id| {
                let mut app = app.borrow_mut();
                app.favorites.retain(|member| member.user_id != user_id);
                app.watcher.forget(user_id);
                app.save();
            }
        }),
        toggle_bell: Box::new({
            let app = Rc::clone(&app);
            move |user_id| {
                let mut app = app.borrow_mut();
                let armed = app.watcher.toggle_bell(user_id);
                app.save();
                armed
            }
        }),
        rows: Box::new({
            let app = Rc::clone(&app);
            move || app.borrow().rows()
        }),
    }));

    // Sans indicateur (bibliothèque absente), l'application continue sans icône mais les alertes fonctionnent.
    let tray = Rc::new(RefCell::new(Tray::new({
        let (app, window) = (Rc::clone(&app), Rc::clone(&window));
        move || {
            window.present();
            app.borrow().poll();
        }
    })));

    // Une vérification peu après le démarrage, puis une toutes les 5 minutes.
    let first_app = Rc::clone(&app);
    gtk::glib::timeout_add_seconds_local_once(config::FIRST_CHECK_SECONDS, move || {
        first_app.borrow().poll();
    });
    let periodic_app = Rc::clone(&app);
    gtk::glib::timeout_add_seconds_local(config::CHECK_INTERVAL_SECONDS, move || {
        periodic_app.borrow().poll();
        gtk::glib::ControlFlow::Continue
    });

    // Les threads de lecture répondent par des canaux ; la boucle GTK les vide chaque seconde.
    gtk::glib::timeout_add_seconds_local(1, move || {
        while let Ok(result) = members_rx.try_recv() {
            match result {
                Ok(members) => {
                    let mut app = app.borrow_mut();
                    if let Err(error) = store::save_members(&app.config_dir, &members) {
                        eprintln!("Cache des membres impossible : {error}");
                    }
                    app.directory = members;
                }
                Err(error) => eprintln!("Annuaire Slack indisponible : {error}"),
            }
        }
        while let Ok(readings) = readings_rx.try_recv() {
            let mut app = app.borrow_mut();
            // Un favori retiré pendant la lecture ne doit pas revenir dans le watcher.
            let readings: Vec<Reading> = readings
                .into_iter()
                .filter(|reading| {
                    app.favorites
                        .iter()
                        .any(|fav| fav.user_id == reading.user_id)
                })
                .collect();
            for user_id in app.watcher.apply(&readings) {
                if let Some(member) = app.favorites.iter().find(|fav| fav.user_id == user_id) {
                    notify::send(&member.name);
                }
            }
            app.save();
            if let Some(tray) = tray.borrow_mut().as_mut() {
                tray.show(app.watcher.in_call_count());
            }
            drop(app);
            window.refresh();
        }
        gtk::glib::ControlFlow::Continue
    });
    gtk::main();
}

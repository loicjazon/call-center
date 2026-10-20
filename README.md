# call-center

Indicateur de la barre GNOME (Rust + GTK3) : liste vos collègues Slack favoris, indique s'ils sont
en ligne, hors ligne ou **en huddle** (« en call »), et envoie une notification de bureau quand un
collègue en call en sort (cloche 🔔). Vérification toutes les 5 minutes. Projet local, sans dépôt distant.

## Utilisation

- Clic sur l'icône → **Ouvrir la liste…** : champ de recherche (auto-complétion) en haut, favoris dessous.
- 🟢 en ligne · 🔴 en call · ⚪ hors ligne ou inconnu. Clic sur un collègue en call = activer/retirer la cloche 🔔
  (à usage unique : elle se désactive après la notification). ✕ retire un favori.
- « En call » = huddle Slack uniquement (pas Zoom, Meet ni Teams).
- Un statut illisible (réseau, jeton, limite de débit) s'affiche « inconnu », jamais « libre ».

## Mise en place Slack

1. Créer une application sur <https://api.slack.com/apps> (« From scratch »).
2. *OAuth & Permissions* → **User Token Scopes** : `users:read`. Installer l'application dans le workspace.
3. Copier le jeton `xoxp-…` dans `~/.config/call-center/token` puis `chmod 600 ~/.config/call-center/token`
   (l'application refuse un fichier lisible par d'autres).

## Diagnostic

```bash
call-center probe members        # liste « identifiant  nom » des membres
call-center probe <USER_ID>      # réponses brutes de users.info et users.getPresence
```

## Développement

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
timeout 60 cargo run             # démon : toujours borné par timeout
./install-local.sh               # compile, installe dans ~/.local/bin, active l'autostart
```

Dépendance d'exécution : `libayatana-appindicator3-1` ; compilation : `libgtk-3-dev`. Notifications : `notify-send`.
Fichiers : `~/.config/call-center/{token,state.json,users-cache.json}`.

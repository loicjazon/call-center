# Call Center

Sachez si un collègue est en call avant de le déranger, et soyez prévenu quand il a fini.

Outil Linux (Rust + GTK3) qui vit dans la barre supérieure du bureau. Il liste vos collègues Slack
favoris, indique s'ils sont en ligne, hors ligne ou **en huddle**, et envoie une notification de bureau
quand un collègue en call en sort.

Page de présentation : <https://loicjazon.github.io/call-center/>

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/loicjazon/call-center/main/install.sh | sh
```

Sans `sudo` : le binaire est installé dans `~/.local/bin` (empreinte SHA-256 vérifiée) et démarre à chaque
ouverture de session. Pour désinstaller : `… | sh -s -- --uninstall` (votre jeton et vos favoris sont conservés).

**Prérequis :** Ubuntu 22.04 ou plus récent avec un bureau GNOME et l'extension « AppIndicator » (activée par
défaut sur Ubuntu), `libayatana-appindicator3-1` pour l'icône, `libnotify-bin` pour les notifications.

## Mise en place Slack

Call Center lit Slack avec **votre** jeton, en lecture seule. Il faut le créer une fois :

1. Créez une application sur <https://api.slack.com/apps> (*Create New App* → *From scratch*).
2. *OAuth & Permissions* → **User Token Scopes** : ajoutez `users:read`, puis *Install to Workspace*
   (selon votre organisation, un administrateur peut devoir valider l'installation).
3. Copiez le jeton `xoxp-…` dans `~/.config/call-center/token`, lisible par vous seul :

   ```bash
   mkdir -p ~/.config/call-center && (umask 077 && cat > ~/.config/call-center/token)
   # collez le jeton, puis Ctrl+D
   ```

L'application refuse un fichier de jeton lisible par d'autres utilisateurs. Le jeton n'est jamais envoyé
ailleurs que vers l'API Slack.

## Utilisation

- Un clic sur l'icône de la barre → **Ouvrir la liste…** : un champ de recherche (accents et casse ignorés)
  pour ajouter un collègue, et la liste des favoris, triée par ordre alphabétique, avec leur avatar.
- 🟢 en ligne · 🔴 en call · ⚪ hors ligne. Ouvrir la liste relit les statuts ; en attendant, ils s'affichent
  « inconnu » plutôt qu'avec une valeur périmée. Un statut illisible (réseau, jeton, limite de débit) reste
  « inconnu », jamais « libre ».
- Un clic sur un collègue **en call** arme la cloche 🔔. Les collègues avec une cloche sont revérifiés chaque
  minute ; quand ils sortent de leur huddle, une notification (avec leur avatar et un son) reste affichée
  jusqu'à ce que vous la fermiez, puis la cloche se désarme. ✕ retire un favori.
- L'icône de la barre indique le nombre de cloches armées (🔔 2), et rien quand il n'y en a pas.

« En call » signifie **huddle Slack** (y compris un appel entre deux personnes en message privé). Un appel
Zoom, Teams ou Meet n'apparaît que si l'outil met lui-même le statut Slack à jour.

## Fichiers

| Fichier | Contenu |
|---|---|
| `~/.config/call-center/token` | Le jeton Slack (`0600`) |
| `~/.config/call-center/state.json` | Favoris et cloches armées |
| `~/.config/call-center/users-cache.json` | Annuaire mis en cache (rafraîchi après 24 h) |
| `~/.cache/call-center/avatars/` | Photos de profil |

Un fichier d'état illisible n'est jamais écrasé : il est conservé en `.bak` et la liste repart vide.

## Diagnostic

```bash
call-center probe members     # liste « identifiant  nom » des membres du workspace
call-center probe <USER_ID>   # réponses brutes de users.info et users.getPresence
```

## Développement

```bash
cargo test                                  # tests unitaires
cargo clippy --all-targets -- -D warnings   # doit rester sans avertissement
cargo fmt --check
cargo build --release                       # binaire : target/release/call-center
timeout 60 cargo run                        # c'est un démon : toujours le borner
```

Compiler demande les en-têtes GTK3 : `sudo apt install libgtk-3-dev`.

| Module | Rôle |
|---|---|
| `slack.rs` | Lecture des réponses de l'API (pure, testée) |
| `client.rs` | Appels HTTP vers Slack, en lecture seule |
| `watcher.rs` | Statuts et cloches : la règle d'alerte (pure, testée) |
| `schedule.rs`, `gate.rs` | Qui vérifier à chaque minute, une seule vérification en vol |
| `directory.rs` | Recherche et tri des collègues |
| `store.rs`, `config.rs` | Fichiers locaux et jeton |
| `avatar.rs` | Cache des photos (téléchargées sans le jeton) |
| `notify.rs`, `tray.rs`, `window.rs`, `main.rs` | Notification, indicateur de barre, fenêtre, assemblage |

Une version se publie en poussant un tag `vX.Y.Z` égal à la version de `Cargo.toml` : le workflow compile
dans Ubuntu 22.04 et publie le binaire avec son empreinte SHA-256.

## Licence

[MIT](LICENSE)

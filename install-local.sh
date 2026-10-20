#!/bin/sh
# Installation locale de call-center : compile, copie le binaire et active le démarrage automatique.
# Aucun téléchargement : tout reste sur cette machine.
set -eu

cd "$(dirname "$0")"
cargo build --release

bin_dir="${HOME}/.local/bin"
autostart_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/autostart"
mkdir -p "${bin_dir}" "${autostart_dir}"
install -m 755 target/release/call-center "${bin_dir}/call-center"

cat > "${autostart_dir}/call-center.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Call Center
Comment=Statut des collègues Slack (en call ou non)
Exec=${bin_dir}/call-center
Terminal=false
X-GNOME-Autostart-enabled=true
DESKTOP

echo "Installé dans ${bin_dir}/call-center (démarrage automatique activé)."
echo "Pensez à placer le jeton Slack dans ${XDG_CONFIG_HOME:-${HOME}/.config}/call-center/token (chmod 600)."

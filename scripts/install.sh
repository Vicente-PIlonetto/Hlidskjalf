#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${HOME}/.local/bin"
data_dir="${XDG_DATA_HOME:-${HOME}/.local/share}"
icon_dir="${data_dir}/icons/hicolor/128x128/apps"
applications_dir="${data_dir}/applications"
service_dir="${HOME}/.config/systemd/user"

config_dir="${XDG_CONFIG_HOME:-${HOME}/.config}"

# A source checkout is built first; a release archive already ships target/release.
if [[ -f "${repo_root}/Cargo.toml" ]]; then
  "${repo_root}/scripts/build.sh"
fi

# Upgrade from the project's former name, Vortal: keep identity, pairing, and settings.
if command -v systemctl >/dev/null 2>&1 && [[ -f "${service_dir}/vortal-input.service" ]]; then
  systemctl --user disable --now vortal-input.service >/dev/null 2>&1 || true
fi
rm -f "${service_dir}/vortal-input.service" "${bin_dir}/vortal" "${bin_dir}/vortald" \
  "${bin_dir}/vortal-desktop" "${applications_dir}/vortal.desktop" "${icon_dir}/com.vortal.desktop.png"
for base in "${data_dir}" "${config_dir}"; do
  if [[ -d "${base}/vortal" && ! -e "${base}/hlidskjalf" ]]; then
    mv "${base}/vortal" "${base}/hlidskjalf"
  fi
done

install -Dm755 "${repo_root}/target/release/hlidskjalf" "${bin_dir}/hlidskjalf"
install -Dm755 "${repo_root}/target/release/hlidskjalfd" "${bin_dir}/hlidskjalfd"
install -Dm755 "${repo_root}/target/release/hlidskjalf-desktop" "${bin_dir}/hlidskjalf-desktop"
install -Dm644 "${repo_root}/crates/hlidskjalf-desktop/assets/hlidskjalf.png" \
  "${icon_dir}/com.hlidskjalf.desktop.png"

desktop_file="${applications_dir}/hlidskjalf.desktop"
install -Dm644 "${repo_root}/scripts/hlidskjalf.desktop" "${desktop_file}"
install -Dm644 "${repo_root}/scripts/hlidskjalf-input.service" \
  "${service_dir}/hlidskjalf-input.service"
sed -i \
  -e "s|^Exec=.*|Exec=${bin_dir}/hlidskjalf-desktop|" \
  -e "s|^Icon=.*|Icon=${icon_dir}/com.hlidskjalf.desktop.png|" \
  "${desktop_file}"

if command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then
  systemctl --user daemon-reload
  if [[ ! -f "${config_dir}/hlidskjalf/startup-enabled" ]] || [[ "$(cat "${config_dir}/hlidskjalf/startup-enabled")" == "1" ]]; then
    systemctl --user enable hlidskjalf-input.service
  fi
  if [[ -f "${data_dir}/hlidskjalf/identity/key.der" && -f "${data_dir}/hlidskjalf/peer.der" ]]; then
    systemctl --user restart hlidskjalf-input.service
  fi
fi

if ! id -nG | tr ' ' '\n' | grep -qx input; then
  printf '\nPara controlar outro computador a partir deste Linux, adicione seu usuário ao grupo input:\n  sudo usermod -aG input %s\nDepois saia e entre novamente na sessão.\n' "${USER}"
fi

printf '\nHliðskjálf instalado. Abra pelo menu de aplicativos ou execute:\n  %s\n' \
  "${bin_dir}/hlidskjalf-desktop"

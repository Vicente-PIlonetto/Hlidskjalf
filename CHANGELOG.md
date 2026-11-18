# Changelog

## [0.1.0] - 2026-11-18

First release as **Hliðskjálf** (ᚼᛚᛁᚦᛋᚴᛁᛅᛚᚠ), formerly Vortal.

### Added

- Share one mouse and keyboard across computers through a portal at a chosen screen edge, over QUIC with pinned certificates.
- Pair by ID: the controlled computer shows its ID and approves the request; receivers are found on the local network or over Tailscale.
- Windows app: tray, start with Windows, clipboard text and images, Linux audio playback, and automatic pt-BR or English.
- Windows as a receiver: another computer can control a Windows PC, including mouse, keyboard with dead keys, and clipboard text.
- Linux controls Windows: the Linux app pairs by ID and opens a portal at a Hyprland monitor edge, sending physical keys so the Windows layout picks characters.
- Portal editor: drag the other computer's screen next to any Windows monitor, like Windows display settings.
- Norse runestone theme: carved hagall rune, runic name, runestone band, and section runes.
- Releases publish a Windows installer and a Linux archive.

### Fixed

- AltGr characters and dead keys from Windows layouts reach the receiver.
- Windows receivers are found over Tailscale.

### Changed

- The project is renamed Hliðskjálf. Binaries are `hlidskjalf`, `hlidskjalfd`, `hlidskjalf-desktop`, and `hlidskjalf-motion`; earlier Vortal data, settings, and installs are migrated automatically. Both computers must run this version to find each other by ID.
- Dependabot is no longer configured.

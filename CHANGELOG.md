# Changelog

## [Unreleased]

### Added

- A gold glow runs along the portal edge when the cursor crosses into the other computer, and spreads back out when control returns. It follows Windows' "Animation effects" setting.
- Gold accent (Draupnir gold, against the silver grays of Valaskjálf) on the brand, active page, primary actions, switches, and the portal marker, in the Windows and Linux apps.
- The portal editor redraws itself when Windows reports a new monitor layout.

### Changed

- Portal crossing is pure geometry with tests for side-by-side, stacked, mismatched, and negative-coordinate monitor layouts.
- CI builds with the pinned Rust 1.88 and retries Ubuntu package downloads.

### Fixed

- The release workflow attaches its files to a release that already exists for the tag instead of failing.

## [0.1.1] - 2026-11-19

### Fixed

- Windows monitors are read live in physical pixels and identified by their Windows number, so the portal editor matches Settings › Display.
- The cursor moves freely between local Windows monitors; only an edge with no monitor beyond it opens the portal.
- Cursor capture and the Windows receiver use per-monitor DPI awareness, so scaled monitors no longer shift edges.
- Releases include the Windows installer (`.exe`); the 0.1.0 tag was cut before the release workflow existed.

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

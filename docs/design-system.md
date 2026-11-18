# Design system

The Linux desktop window uses GTK widgets implemented in `crates/hlidskjalf-ui/src/linux_native.rs`; it shows the device ID, pending pairing request, and receiver settings. The Windows package still uses Electron and the simplified interface in `crates/hlidskjalf-ui/ui`. Run `./scripts/install.sh` to install the Linux app and icon under `~/.local`. `./scripts/serve-ui.sh` remains available for browser previews.

- Dark by default, with system theme selected on first launch and an optional light theme.
- Monochrome 11-step grayscale tokens in one CSS variables file; no hardcoded colors elsewhere.
- Flat Metro tiles on an 8 px grid. No shadows, gradients, decorative transparency, or rounded corners.
- Selawik with Segoe UI and system sans-serif fallbacks. Use typographic hierarchy and luminance for state.
- Short labels and one-word states. UI copy is localized in `pt-BR` and `en`.
- Animate only `transform` and `opacity`; honor `prefers-reduced-motion` and keep all transitions under 500 ms.
- Visible keyboard focus, full keyboard navigation, and WCAG AA contrast.
- Keep UI rendering in a separate process from the input daemon.

## Norse motifs

Hliðskjálf is Odin's high seat, from which he sees every world. The interface borrows from Viking-age runestones while keeping the flat grayscale system above.

- **Runes:** Younger Futhark glyphs set in `--font-rune` (Segoe UI Historic on Windows, Noto Sans Runic on Linux). The name is written ᚼᛚᛁᚦᛋᚴᛁᛅᛚᚠ; the brand mark is the hagall rune ᚼ.
- **Section runes:** each eyebrow carries a rune through `data-rune`: ᚱ reið (journey) for pairing and portals, ᛘ maðr (people) for devices, ᛏ Týr (order) for settings, ᚬ óss (Odin) for about, ᚾ nauð (need) for pairing requests.
- **Runestone band:** below the top bar, "Óðinn sér alla heima" (Odin sees every world) repeats as ᚢᚦᛁᚾ᛫ᛋᛁᚱ᛫ᛅᛚᛅ᛫ᚼᛅᛁᛘᛅ᛬ in a low-contrast band.
- **Carved corners:** tiles and panels show two square corner cuts that open slightly on hover. The connection tile carries a large ᚼ seal in a solid gray, never transparency.
- **Motion:** the brand rune is carved stroke by stroke on launch and pulses while connecting; a single "Bifröst" ripple leaves the status indicator on connect; section runes turn into place; the band drifts slowly. All of it uses `transform` and `opacity` and stops under `prefers-reduced-motion`. The GTK app, which has no CSS transforms, uses opacity only.


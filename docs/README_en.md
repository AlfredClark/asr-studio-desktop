English | [简体中文](../README.md)

# ASR Studio

A local speech-recognition desktop app built on Tauri v2 + SvelteKit 2 + Svelte 5. Recording, models, and recognition all run on-device and work fully offline; the network is only used to fetch models and check for updates.

## Features

- **Fully offline recognition**: sherpa-onnx based; recording, model loading, and decoding never leave the machine.
- **Recording & transcription**: input device selection, start/stop recording, audio file transcription, cancellation and timeout settlement.
- **Model management**: on-demand download / cancel / verify / delete, custom model directory, on-disk usage stats.
- **Lexicon**: group and entry management, batch enable / copy / move / delete, group import/export, pinyin suggestions, quick-add from selected text.
- **Directives**: replay a key sequence (press/release balance-checked) when trigger text matches, with a 104-key recording keyboard; keys unsupported on the current platform are rejected before replay.
- **Result output**: IME injection / clipboard / simulated-paste channels switched by config with automatic fallback; a focus pre-check writes straight to the clipboard when no editable focus exists.
- **Global shortcuts**: voice, quick-lexicon, and directive channels, reconciled against the running service state.
- **Desktop integration**: system tray, frameless custom title bar, autostart, window-state restore, single-instance focus + deep links (`asr-studio://`).
- **Auto updates**: wired to the GitHub Releases `latest.json`, with check, download-and-install, and restart.
- **Bilingual UI & appearance**: `en` / `zh-CN`; light/dark mode, color themes, adjustable font family / weight / size.
- **Crash safety nets**: frontend `ErrorBoundary` plus a backend panic hook persisted to disk, so failures never go missing silently.

## Tech stack

- Frontend: SvelteKit 2 (SPA, `adapter-static`) + Svelte 5 (runes) + Tailwind CSS v4 + shadcn-svelte + Paraglide (`en` / `zh-CN`) + tauri-specta generated frontend/backend contract bindings.
- Backend: Tauri v2 + Rust (edition 2024, toolchain pinned to `1.99.1`); speech via `sherpa-onnx`, recording via `cpal`, pinyin via `pinyin`, key replay via `enigo`.
- Tooling: `pnpm@12.14.0` (Node `>= 24`); both `pnpm-lock.yaml` and `Cargo.lock` are committed.

## Quickstart

```bash
# Prereqs: Node >= 24, pnpm@12.14.0, Rust 1.99.1
# Linux additionally needs system deps (WebKitGTK etc.), see the Dependencies step in .github/workflows/release.yml

pnpm install --frozen-lockfile   # install from lockfiles

pnpm tauri:dev                   # full desktop loop (Vite on fixed port 1420 + Tauri window)
pnpm dev                         # frontend only (Tauri commands degrade gracefully in a browser)

pnpm format                      # required after every change: Prettier + ESLint + cargo fmt
pnpm validate                    # full gate: test + lint + check (cargo test regenerates bindings)
```

After editing `messages/` copy, always run `pnpm i18n:compile`, otherwise the build and CI will fail.

## Pages

| Route         | Contents                                                                                        |
| ------------- | ----------------------------------------------------------------------------------------------- |
| `/`           | Workbench: three-step onboarding without a ready model, else status cards + recent recognitions |
| `/service`    | Speech service: start/stop the service, recording and transcription                             |
| `/models`     | Models: download, verify, delete, custom directory                                              |
| `/lexicon`    | Lexicon: group and entry management, batch ops, import/export                                   |
| `/directives` | Directives: trigger text and key-sequence management, replay                                    |
| `/settings`   | Settings: general, appearance, and output preferences                                           |
| `/about`      | About: app / project / platform info, diagnostics, updates                                      |

## Layout

```text
src/                 Frontend SvelteKit SPA: routes/ (pages) · components/ (common / layout / shadcn-svelte)
                     · libs/ (commands contract chain · i18n · hooks · navigation · utils) · tests/
src-tauri/           Backend: src/{commands (thin wrappers) · cores (shared capabilities) · features (business) · plugins/}
                     · locales/ · capabilities/ · tauri.conf.json
docs/                ARCHITECTURE.md (structure & invariants) · WORKFLOW.md (commands & flows) · README_en.md (this file)
static/              Copied-verbatim assets; .github/workflows/ (ci.yml / release.yml)
```

One-way layering: `Svelte components → libs/commands → commands/ → features/ · cores/`, and `cores` must never depend on `features`. See `docs/ARCHITECTURE.md` for the full tree and boundaries.

## Config & releases

- Three version numbers move together: `package.json` + `src-tauri/tauri.conf.json` + `src-tauri/Cargo.toml`, bumped via `pnpm release` (bumpp); tagging `vX.Y.Z` triggers `release.yml` to bundle (deb / AppImage / rpm / dmg / msi / nsis) with update signatures and a changelog.
- New external URLs must be synced to both `capabilities/*.json` and the `tauri.conf.json` CSP; the updater private key only travels via environment variables, never the repo.
- Commits follow Conventional Commits (English, lowercase `type`) and are made manually by developers; the command contract and verification matrix live in `docs/WORKFLOW.md`.

## Docs

- `AGENTS.md`: standing dev rules (red lines, architecture invariants, workflow gates).
- `docs/ARCHITECTURE.md`: architecture, directory ownership, and design invariants.
- `docs/WORKFLOW.md`: everyday commands, i18n / command-contract / release flows, and the verification matrix.
- `../README.md`: the Chinese primary version of this file.

## License

GPL-3.0-only, by Alfred Clark. Evaluate GPL copyleft obligations before any closed-source distribution.

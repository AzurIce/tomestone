**English** | [中文](README.zh.md)

# Tomestone

> **WIP** — This project is in early development. Features and APIs are subject to change.

A Rust-based FFXIV equipment model viewer with 3D rendering and dye system support.

![screenshot](assets/screenshot.png)

## Features

- Browse equipment list from game data, filtered by slot (head / body / gloves / legs / feet)
- Load and render 3D equipment models (MDL) with textures (TEX)
- Dawntrail (7.0+) support: ColorTable + `_id.tex` lookup-based coloring
- Dye preview: select a stain and see real-time color changes (powered by STM staining templates)

## Tech Stack

- [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) / [egui](https://github.com/emilk/egui) — UI framework
- [wgpu](https://github.com/gfx-rs/wgpu) — GPU rendering
- [physis](https://github.com/redstrate/Physis) — FFXIV game data parsing

## Roadmap

Currently only basic equipment rendering and dye preview are supported. Future plans may include better rendering (normal maps, lighting, PBR materials), extending to accessories, weapons, furniture, mounts and minions, as well as multi-slot character model assembly. A more ambitious idea would be a plugin for interacting with the FFXIV game client itself, such as glamour dresser synchronization.

## Build

Requires Rust toolchain.

```bash
cargo build --release
```

# Fylins

A very basic but fast terminal file browser.

## Features

- Vim-style navigation (j/k/h/l)
- File preview with syntax highlighting
- Git status indicators
- Search/filter files
- File operations (create, copy, cut, paste, rename, delete)
- Hidden files toggle
- Path jumping

## Installation

```sh
cargo build --release
```

Binary: `target/release/fylins.exe`

## Usage

```sh
fylins [path]
```

## Keybindings

| Key | Action |
|-----|--------|
| `j/k` | Navigate down/up |
| `Enter/l` | Open directory |
| `h/Backspace` | Go to parent |
| `` ` `` | Go to start directory |
| `/` | Search/filter |
| `H` | Toggle hidden files |
| `c` | Copy file |
| `x` | Cut file |
| `v` | Paste file |
| `n` | New file |
| `N` | New folder |
| `y` | Yank (copy) path |
| `r` | Rename |
| `d` | Delete |
| `o` | Open with default app |
| `p` | Jump to path |
| `q/Esc` | Quit |

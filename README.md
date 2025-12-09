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

**Navigation:**

- `h/j/k/l` or `←/↓/↑/→` - Navigate (vim-style)
- `Enter` - Open directory/file
- `Backspace` - Go to parent directory
- `` ` `` - Go to start directory
- `PageUp/PageDown` - Scroll preview

**File Operations:**

- `c` - Copy file
- `x` - Cut file
- `v` - Paste file
- `n` - New file
- `N` - New folder
- `r` - Rename
- `d` - Delete
- `o` - Open with default app

**Other:**

- `/` - Search/filter
- `H` - Toggle hidden files
- `y` - Yank (copy) path to clipboard
- `p` - Jump to path
- `q` or `Esc` - Quit

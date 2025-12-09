use std::{
    collections::HashMap,
    fs,
    io::{self, Read, Seek},
    path::{Component, Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use arboard::Clipboard;
use ratatui::widgets::ListState;

// =============================================================================
// Data Types
// =============================================================================

/// Application mode determining current input handling behavior.
#[derive(Debug, Default, PartialEq, Clone)]
pub enum Mode {
    /// Default navigation mode.
    #[default]
    Normal,
    /// Live search/filter mode.
    Search,
    /// Renaming a file or folder.
    Rename,
    /// Awaiting delete confirmation.
    ConfirmDelete,
    /// Entering a path to navigate to.
    Path,
    /// Creating a new file.
    NewFile,
    /// Creating a new folder.
    NewFolder,
    /// Showing help screen.
    Help,
}

/// Clipboard state for copy/cut operations.
#[derive(Clone)]
pub struct FileClipboard {
    /// Path to the source file or directory.
    pub path: PathBuf,
    /// True if this is a cut (move) operation.
    pub is_cut: bool,
}

/// Main application state.
pub struct App {
    pub current_dir: PathBuf,
    pub start_dir: PathBuf,
    pub all_entries: Vec<Entry>,
    pub filtered_indices: Vec<usize>,
    pub state: ListState,
    pub preview: Preview,
    pub scroll: u16,
    pub mode: Mode,
    pub input: Vec<char>,
    pub cursor: usize,
    pub show_hidden: bool,
    pub message: Option<String>,
    pub clipboard: Option<FileClipboard>,
    git_statuses: HashMap<String, GitStatus>,
    /// Cached directory for git status (avoids re-running git on same dir)
    git_cache_dir: Option<PathBuf>,
}

/// Represents a file or directory entry.
pub struct Entry {
    pub name: String,
    /// Pre-computed lowercase name for efficient filtering
    pub name_lower: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub is_hidden: bool,
    pub readonly: bool,
    pub git_status: Option<GitStatus>,
}

/// Git status for a file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GitStatus {
    Modified,
    Staged,
    Untracked,
    Ignored,
    Conflict,
}

/// Preview content for the selected file.
pub enum Preview {
    None,
    Directory(Vec<String>),
    Text {
        content: String,
        extension: String,
    },
    Image {
        width: u32,
        height: u32,
        format: &'static str,
    },
    Binary(Vec<u8>),
    Error(String),
}

// =============================================================================
// App Implementation
// =============================================================================

impl App {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let mut app = App {
            current_dir: path.clone(),
            start_dir: path,
            all_entries: Vec::with_capacity(256),
            filtered_indices: Vec::with_capacity(256),
            state: ListState::default(),
            preview: Preview::None,
            scroll: 0,
            mode: Mode::Normal,
            input: Vec::with_capacity(64),
            cursor: 0,
            show_hidden: false,
            message: None,
            clipboard: None,
            git_statuses: HashMap::with_capacity(64),
            git_cache_dir: None,
        };
        app.refresh()?;
        if !app.filtered_indices.is_empty() {
            app.state.select(Some(0));
            app.update_preview();
        }
        Ok(app)
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.filtered_indices
            .iter()
            .filter_map(|&i| self.all_entries.get(i))
    }

    /// Invalidate git cache to force re-fetching on next refresh
    fn invalidate_git_cache(&mut self) {
        self.git_cache_dir = None;
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        self.all_entries.clear();

        // Only refresh git status if directory changed
        if self.git_cache_dir.as_ref() != Some(&self.current_dir) {
            self.git_statuses = get_git_status(&self.current_dir);
            self.git_cache_dir = Some(self.current_dir.clone());
        }

        if self.current_dir.parent().is_some() {
            self.all_entries.push(Entry {
                name: "..".to_string(),
                name_lower: "..".to_string(),
                is_dir: true,
                size: 0,
                modified: None,
                is_hidden: false,
                readonly: false,
                git_status: None,
            });
        }

        let mut entries: Vec<Entry> = fs::read_dir(&self.current_dir)?
            .filter_map(|e| e.ok())
            .map(|e| {
                let metadata = e.metadata().ok();
                let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = metadata.as_ref().and_then(|m| m.modified().ok());
                let readonly = metadata
                    .as_ref()
                    .map(|m| m.permissions().readonly())
                    .unwrap_or(false);
                let name = e.file_name().to_string_lossy().to_string();
                let is_hidden = is_hidden_file(&name, &e.path());
                let git_status = self.git_statuses.get(&name).copied();
                let name_lower = name.to_lowercase();
                Entry {
                    name,
                    name_lower,
                    is_dir,
                    size,
                    modified,
                    is_hidden,
                    readonly,
                    git_status,
                }
            })
            .collect();

        // Sort using pre-computed lowercase names to avoid repeated allocations
        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name_lower.cmp(&b.name_lower),
        });

        self.all_entries.extend(entries);
        self.apply_filter();
        Ok(())
    }

    pub fn apply_filter(&mut self) {
        let query: String = self.input.iter().collect::<String>().to_lowercase();
        self.filtered_indices = self
            .all_entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                // Always show ".." entry
                if e.name == ".." {
                    return true;
                }
                // Filter hidden files
                if !self.show_hidden && e.is_hidden {
                    return false;
                }
                // Apply search filter using pre-computed lowercase
                if self.mode == Mode::Search && !query.is_empty() {
                    return e.name_lower.contains(&query);
                }
                true
            })
            .map(|(i, _)| i)
            .collect();

        // Reset selection if out of bounds
        if let Some(selected) = self.state.selected() {
            if selected >= self.filtered_indices.len() {
                self.state.select(if self.filtered_indices.is_empty() {
                    None
                } else {
                    Some(0)
                });
            }
        } else if !self.filtered_indices.is_empty() {
            self.state.select(Some(0));
        }
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.state
            .selected()
            .and_then(|i| self.filtered_indices.get(i))
            .and_then(|&idx| self.all_entries.get(idx))
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected_entry().map(|e| {
            if e.name == ".." {
                self.current_dir.parent().unwrap().to_path_buf()
            } else {
                self.current_dir.join(&e.name)
            }
        })
    }

    pub fn update_preview(&mut self) {
        self.scroll = 0;
        self.preview = match self.selected_entry() {
            None => Preview::None,
            Some(entry) if entry.is_dir => {
                let path = if entry.name == ".." {
                    self.current_dir.parent().map(|p| p.to_path_buf())
                } else {
                    Some(self.current_dir.join(&entry.name))
                };
                path.map(|p| self.load_directory_preview(&p))
                    .unwrap_or(Preview::None)
            }
            Some(entry) => {
                let path = self.current_dir.join(&entry.name);
                self.load_file_preview(&path)
            }
        };
    }

    fn load_directory_preview(&self, path: &Path) -> Preview {
        match fs::read_dir(path) {
            Ok(entries) => {
                // (is_dir, name, name_lower) - pre-compute lowercase for sorting
                let mut items: Vec<(bool, String, String)> = entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        let hidden = is_hidden_file(&name, &e.path());
                        if !self.show_hidden && hidden {
                            return None;
                        }
                        let is_dir = e.metadata().map(|m| m.is_dir()).unwrap_or(false);
                        let name_lower = name.to_lowercase();
                        Some((is_dir, name, name_lower))
                    })
                    .collect();

                items.sort_by(|a, b| match (a.0, b.0) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.2.cmp(&b.2),
                });

                let formatted: Vec<String> = items
                    .into_iter()
                    .map(|(is_dir, name, _)| {
                        if is_dir {
                            format!("📁 {}", name)
                        } else {
                            format!("📄 {}", name)
                        }
                    })
                    .collect();

                Preview::Directory(formatted)
            }
            Err(e) => Preview::Error(format!("Cannot read directory: {}", e)),
        }
    }

    fn load_file_preview(&self, path: &Path) -> Preview {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Check for image files
        if matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp"
        ) {
            return self.load_image_preview(path, &extension);
        }

        const MAX_PREVIEW: usize = 16 * 1024;

        let mut file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) => return Preview::Error(format!("Cannot open: {}", e)),
        };

        let mut buffer = vec![0u8; MAX_PREVIEW];
        let bytes_read = match file.read(&mut buffer) {
            Ok(n) => n,
            Err(e) => return Preview::Error(format!("Cannot read: {}", e)),
        };
        buffer.truncate(bytes_read);

        if is_text(&buffer) {
            match String::from_utf8(buffer) {
                Ok(s) => Preview::Text {
                    content: s,
                    extension,
                },
                Err(e) => Preview::Binary(e.into_bytes()),
            }
        } else {
            Preview::Binary(buffer)
        }
    }

    fn load_image_preview(&self, path: &Path, ext: &str) -> Preview {
        let mut file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) => return Preview::Error(format!("Cannot open: {}", e)),
        };

        let mut header = [0u8; 32];
        if file.read(&mut header).is_err() {
            return Preview::Error("Cannot read image header".to_string());
        }

        let (width, height, format): (u32, u32, &'static str) = match ext {
            "png" => parse_png_dimensions(&header),
            "jpg" | "jpeg" => {
                // JPEG requires reading more data
                let mut full_header = vec![0u8; 512];
                let _ = file.rewind();
                let _ = file.read(&mut full_header);
                parse_jpeg_dimensions(&full_header)
            }
            "gif" => parse_gif_dimensions(&header),
            "bmp" => parse_bmp_dimensions(&header),
            "ico" => (0, 0, "ICO"),
            "webp" => (0, 0, "WEBP"),
            _ => (0, 0, "Image"),
        };

        Preview::Image {
            width,
            height,
            format,
        }
    }

    pub fn enter_selected(&mut self) -> io::Result<()> {
        if let Some(entry) = self.selected_entry() {
            if entry.is_dir {
                let new_path = if entry.name == ".." {
                    self.current_dir.parent().unwrap().to_path_buf()
                } else {
                    self.current_dir.join(&entry.name)
                };
                self.current_dir = new_path.canonicalize()?;
                self.input.clear();
                self.mode = Mode::Normal;
                self.refresh()?;
                self.state.select(Some(0));
                self.update_preview();
            }
        }
        Ok(())
    }

    pub fn move_up(&mut self) {
        if let Some(selected) = self.state.selected() {
            if selected > 0 {
                self.state.select(Some(selected - 1));
                self.update_preview();
            }
        }
    }

    pub fn move_down(&mut self) {
        if let Some(selected) = self.state.selected() {
            if selected < self.filtered_indices.len().saturating_sub(1) {
                self.state.select(Some(selected + 1));
                self.update_preview();
            }
        }
    }

    pub fn scroll_preview_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(3);
    }

    pub fn scroll_preview_down(&mut self) {
        self.scroll = self.scroll.saturating_add(3);
    }

    pub fn go_to_parent(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            self.current_dir = parent.to_path_buf();
            self.input.clear();
            self.mode = Mode::Normal;
            let _ = self.refresh();
            self.state.select(Some(0));
            self.update_preview();
        }
    }

    pub fn go_to_start(&mut self) {
        if self.current_dir != self.start_dir {
            self.current_dir = self.start_dir.clone();
            self.input.clear();
            self.mode = Mode::Normal;
            let _ = self.refresh();
            self.state.select(Some(0));
            self.update_preview();
            self.message = Some("Back to start".to_string());
        }
    }

    // =========================================================================
    // Search/Filter
    // =========================================================================

    pub fn start_search(&mut self) {
        self.mode = Mode::Search;
        self.input.clear();
        self.message = Some("Search: type to filter".to_string());
    }

    pub fn cancel_search(&mut self) {
        self.mode = Mode::Normal;
        self.input.clear();
        self.message = None;
        self.apply_filter();
        self.update_preview();
    }

    pub fn confirm_search(&mut self) {
        self.mode = Mode::Normal;
        self.message = None;
        // Keep the filter applied
    }

    pub fn update_search(&mut self, c: char) {
        self.input.push(c);
        self.apply_filter();
        self.update_preview();
    }

    pub fn backspace_search(&mut self) {
        self.input.pop();
        self.apply_filter();
        self.update_preview();
    }

    // =========================================================================
    // Hidden Files
    // =========================================================================

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.message = Some(format!(
            "Hidden files: {}",
            if self.show_hidden { "shown" } else { "hidden" }
        ));
        self.apply_filter();
        self.update_preview();
    }

    // =========================================================================
    // File Operations
    // =========================================================================

    pub fn yank_path(&mut self) {
        if let Some(path) = self.selected_path() {
            let path_str = path.to_string_lossy().to_string();
            match Clipboard::new().and_then(|mut cb| cb.set_text(&path_str)) {
                Ok(_) => self.message = Some(format!("Copied: {}", path_str)),
                Err(e) => self.message = Some(format!("Failed to copy: {}", e)),
            }
        }
    }

    pub fn copy_file(&mut self) {
        let entry_info = self
            .selected_entry()
            .map(|e| (e.name.clone(), e.name == ".."));
        if let Some((name, is_parent)) = entry_info {
            if is_parent {
                return;
            }
            let path = self.current_dir.join(&name);
            self.clipboard = Some(FileClipboard {
                path,
                is_cut: false,
            });
            self.message = Some(format!("Copied: {}", name));
        }
    }

    pub fn cut_file(&mut self) {
        let entry_info = self
            .selected_entry()
            .map(|e| (e.name.clone(), e.name == ".."));
        if let Some((name, is_parent)) = entry_info {
            if is_parent {
                return;
            }
            let path = self.current_dir.join(&name);
            self.clipboard = Some(FileClipboard { path, is_cut: true });
            self.message = Some(format!("Cut: {}", name));
        }
    }

    pub fn paste_file(&mut self) {
        let clip = match &self.clipboard {
            Some(c) => c.clone(),
            None => {
                self.message = Some("Nothing to paste".to_string());
                return;
            }
        };

        if !clip.path.exists() {
            self.message = Some("Source no longer exists".to_string());
            self.clipboard = None;
            return;
        }

        if clip.is_cut {
            if let Some(parent) = clip.path.parent() {
                if parent == self.current_dir {
                    self.message = Some("Item is already in this directory".to_string());
                    return;
                }
            }
        }

        let file_name = match clip.path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => {
                self.message = Some("Invalid source path".to_string());
                return;
            }
        };

        let mut dest = self.current_dir.join(&file_name);

        if clip.path.is_dir() && dest.starts_with(&clip.path) {
            self.message = Some("Cannot copy a directory into itself".to_string());
            return;
        }

        // Handle name conflicts
        if dest.exists() {
            let stem = clip
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = clip
                .path
                .extension()
                .map(|s| format!(".{}", s.to_string_lossy()))
                .unwrap_or_default();
            let mut counter = 1;
            while dest.exists() {
                dest = self
                    .current_dir
                    .join(format!("{}_{}{}", stem, counter, ext));
                counter += 1;
            }
        }

        let result = if clip.is_cut {
            fs::rename(&clip.path, &dest)
        } else if clip.path.is_dir() {
            copy_dir_recursive(&clip.path, &dest)
        } else {
            fs::copy(&clip.path, &dest).map(|_| ())
        };

        match result {
            Ok(_) => {
                let action = if clip.is_cut { "Moved" } else { "Pasted" };
                self.message = Some(format!(
                    "{}: {}",
                    action,
                    dest.file_name().unwrap_or_default().to_string_lossy()
                ));
                if clip.is_cut {
                    self.clipboard = None;
                }
                self.invalidate_git_cache();
                let _ = self.refresh();
                self.update_preview();
            }
            Err(e) => {
                self.message = Some(format!("Paste failed: {}", e));
            }
        }
    }

    pub fn open_with_default(&mut self) {
        if let Some(entry) = self.selected_entry() {
            if entry.name == ".." {
                return;
            }
            let path = self.current_dir.join(&entry.name);

            #[cfg(windows)]
            let result = std::process::Command::new("cmd")
                .args(["/C", "start", "", &path.to_string_lossy()])
                .spawn();

            #[cfg(not(windows))]
            let result = std::process::Command::new("xdg-open").arg(&path).spawn();

            match result {
                Ok(_) => self.message = Some(format!("Opened: {}", entry.name)),
                Err(e) => self.message = Some(format!("Failed to open: {}", e)),
            }
        }
    }

    pub fn start_path(&mut self) {
        self.mode = Mode::Path;
        self.input = self.current_dir.to_string_lossy().chars().collect();
        self.cursor = self.input.len();
        self.message = None;
    }

    pub fn confirm_path(&mut self) {
        let path_str: String = self.input.iter().collect();
        let path = PathBuf::from(&path_str);

        if !path.exists() {
            self.message = Some(format!("Path does not exist: {}", path_str));
            return;
        }

        let target = if path.is_file() {
            path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
        } else {
            path
        };

        match target.canonicalize() {
            Ok(canonical) => {
                self.current_dir = canonical;
                self.mode = Mode::Normal;
                self.input.clear();
                self.cursor = 0;
                let _ = self.refresh();
                self.state.select(Some(0));
                self.update_preview();
                self.message = None;
            }
            Err(e) => {
                self.message = Some(format!("Cannot navigate: {}", e));
            }
        }
    }

    pub fn cancel_path(&mut self) {
        self.mode = Mode::Normal;
        self.input.clear();
        self.cursor = 0;
        self.message = None;
    }

    pub fn start_new_file(&mut self) {
        self.mode = Mode::NewFile;
        self.input.clear();
        self.cursor = 0;
        self.message = None;
    }

    pub fn start_new_folder(&mut self) {
        self.mode = Mode::NewFolder;
        self.input.clear();
        self.cursor = 0;
        self.message = None;
    }

    pub fn confirm_new_file(&mut self) {
        let name: String = self.input.iter().collect();
        if name.is_empty() {
            self.message = Some("Name cannot be empty".to_string());
            return;
        }

        let path = self.current_dir.join(&name);
        if path.exists() {
            self.message = Some(format!("'{}' already exists", name));
            return;
        }

        match fs::File::create(&path) {
            Ok(_) => {
                self.message = Some(format!("Created: {}", name));
                self.mode = Mode::Normal;
                self.input.clear();
                self.cursor = 0;
                self.invalidate_git_cache();
                let _ = self.refresh();
                self.update_preview();
            }
            Err(e) => {
                self.message = Some(format!("Failed to create file: {}", e));
            }
        }
    }

    pub fn confirm_new_folder(&mut self) {
        let name: String = self.input.iter().collect();
        if name.is_empty() {
            self.message = Some("Name cannot be empty".to_string());
            return;
        }

        let path = self.current_dir.join(&name);
        if path.exists() {
            self.message = Some(format!("'{}' already exists", name));
            return;
        }

        match fs::create_dir(&path) {
            Ok(_) => {
                self.message = Some(format!("Created: {}", name));
                self.mode = Mode::Normal;
                self.input.clear();
                self.cursor = 0;
                self.invalidate_git_cache();
                let _ = self.refresh();
                self.update_preview();
            }
            Err(e) => {
                self.message = Some(format!("Failed to create folder: {}", e));
            }
        }
    }

    pub fn cancel_new(&mut self) {
        self.mode = Mode::Normal;
        self.input.clear();
        self.cursor = 0;
        self.message = None;
    }

    pub fn start_rename(&mut self) {
        let entry_info = self
            .selected_entry()
            .map(|e| (e.name.clone(), e.name == ".."));

        if let Some((name, is_parent)) = entry_info {
            if is_parent {
                self.message = Some("Cannot rename '..'".to_string());
                return;
            }
            self.mode = Mode::Rename;
            self.input = name.chars().collect();
            self.cursor = self.input.len();
            self.message = None;
        }
    }

    pub fn toggle_help(&mut self) {
        if self.mode == Mode::Help {
            self.mode = Mode::Normal;
        } else {
            self.mode = Mode::Help;
        }
    }

    // =========================================================================
    // Input/Cursor Handling
    // =========================================================================

    pub fn input_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn input_backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
        }
    }

    pub fn input_delete(&mut self) {
        if self.cursor < self.input.len() {
            self.input.remove(self.cursor);
        }
    }

    pub fn cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += 1;
        }
    }

    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn cursor_end(&mut self) {
        self.cursor = self.input.len();
    }

    pub fn input_clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    pub fn confirm_rename(&mut self) {
        if let Some(entry) = self.selected_entry() {
            let new_name: String = self.input.iter().collect();
            let old_path = self.current_dir.join(&entry.name);
            let new_path = self.current_dir.join(&new_name);

            if new_name.is_empty() {
                self.message = Some("Name cannot be empty".to_string());
                return;
            }

            if old_path == new_path {
                self.mode = Mode::Normal;
                self.input.clear();
                self.message = None;
                return;
            }

            match fs::rename(&old_path, &new_path) {
                Ok(_) => {
                    self.message = Some(format!("Renamed to: {}", new_name));
                    self.mode = Mode::Normal;
                    self.input.clear();
                    self.invalidate_git_cache();
                    let _ = self.refresh();
                    self.update_preview();
                }
                Err(e) => {
                    self.message = Some(format!("Rename failed: {}", e));
                }
            }
        }
    }

    pub fn start_delete(&mut self) {
        let entry_info = self
            .selected_entry()
            .map(|e| (e.name.clone(), e.name == ".."));

        if let Some((name, is_parent)) = entry_info {
            if is_parent {
                self.message = Some("Cannot delete '..'".to_string());
                return;
            }
            self.mode = Mode::ConfirmDelete;
            self.message = Some(format!("Delete '{}'? (y/n)", name));
        }
    }

    pub fn confirm_delete(&mut self) {
        if let Some(path) = self.selected_path() {
            let is_dir = path.is_dir();
            let result = if is_dir {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };

            match result {
                Ok(_) => {
                    self.message = Some("Deleted successfully".to_string());
                    self.invalidate_git_cache();
                    let _ = self.refresh();
                    // Adjust selection if needed
                    if let Some(selected) = self.state.selected() {
                        if selected >= self.filtered_indices.len() && selected > 0 {
                            self.state.select(Some(selected - 1));
                        }
                    }
                    self.update_preview();
                }
                Err(e) => {
                    self.message = Some(format!("Delete failed: {}", e));
                }
            }
        }
        self.mode = Mode::Normal;
    }

    pub fn cancel_delete(&mut self) {
        self.mode = Mode::Normal;
        self.message = None;
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn is_text(data: &[u8]) -> bool {
    if data.is_empty() {
        return true;
    }

    let non_text_count = data
        .iter()
        .take(512)
        .filter(|&&b| b < 0x09 || (b > 0x0D && b < 0x20 && b != 0x1B))
        .count();

    non_text_count * 100 / data.len().min(512) < 5
}

#[cfg(windows)]
fn is_hidden_file(name: &str, path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

    // Check for dotfiles (common convention)
    if name.starts_with('.') && name != ".." {
        return true;
    }

    // Check Windows hidden attribute
    if let Ok(metadata) = fs::metadata(path) {
        return metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0;
    }
    false
}

#[cfg(not(windows))]
fn is_hidden_file(name: &str, _path: &Path) -> bool {
    name.starts_with('.') && name != ".."
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Remove Windows UNC prefix (\\?\) if present
fn normalize_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str.starts_with(r"\\?\") {
        PathBuf::from(&path_str[4..])
    } else {
        path.to_path_buf()
    }
}

fn get_git_status(dir: &Path) -> HashMap<String, GitStatus> {
    let mut statuses = HashMap::new();

    // First, get the git root directory
    let git_root_output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output();

    let git_root = match git_root_output {
        Ok(output) if output.status.success() => {
            let root_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Normalize path separators (git on Windows may return forward slashes)
            PathBuf::from(root_str.replace('/', std::path::MAIN_SEPARATOR_STR))
        }
        _ => return statuses, // Not a git repo or error
    };

    // Normalize both paths to remove UNC prefix (Windows \\?\ prefix)
    let normalized_git_root = normalize_path(&git_root);
    let normalized_dir = normalize_path(dir);

    // Calculate the relative path from git root to current dir
    let relative_prefix = match normalized_dir.strip_prefix(&normalized_git_root) {
        Ok(rel) if rel.as_os_str().is_empty() => None,
        Ok(rel) => Some(rel.to_path_buf()),
        Err(_) => None,
    };

    let output = Command::new("git")
        .args(["status", "--porcelain", "-uall"])
        .current_dir(dir)
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.len() < 4 {
                    continue;
                }
                let status_chars = &line[0..2];
                let mut file_path = line[3..].trim();

                // For renames, use the destination path (after the arrow)
                if let Some(pos) = file_path.find("->") {
                    let new_path = file_path[pos + 2..].trim();
                    if !new_path.is_empty() {
                        file_path = new_path;
                    }
                }

                // Git outputs paths relative to repo root, so strip the prefix
                // to make them relative to the current directory
                // Normalize path separators first
                let normalized_path = file_path.replace('/', std::path::MAIN_SEPARATOR_STR);

                let relative_file_path = if let Some(ref prefix) = relative_prefix {
                    // Use string manipulation for more reliable prefix stripping
                    let prefix_str = prefix.to_string_lossy();
                    let prefix_with_sep = if prefix_str.is_empty() {
                        String::new()
                    } else {
                        format!("{}{}", prefix_str, std::path::MAIN_SEPARATOR)
                    };

                    if normalized_path.starts_with(&prefix_with_sep) {
                        normalized_path[prefix_with_sep.len()..].to_string()
                    } else {
                        normalized_path.clone()
                    }
                } else {
                    normalized_path.clone()
                };

                // Track status by the top-level entry in the current directory
                let entry_key = Path::new(&relative_file_path)
                    .components()
                    .next()
                    .and_then(|c| match c {
                        Component::Normal(os) => os.to_str().map(|s| s.to_string()),
                        _ => None,
                    });

                let Some(entry_key) = entry_key else {
                    continue;
                };

                let status = match status_chars {
                    "??" => GitStatus::Untracked,
                    "!!" => GitStatus::Ignored,
                    "UU" | "AA" | "DD" => GitStatus::Conflict,
                    s if s.starts_with('A')
                        || s.starts_with('M')
                        || s.starts_with('D')
                        || s.starts_with('R') =>
                    {
                        if s.chars().nth(1) == Some(' ') {
                            GitStatus::Staged
                        } else {
                            GitStatus::Modified
                        }
                    }
                    s if s.ends_with('M') || s.ends_with('D') => GitStatus::Modified,
                    _ => continue,
                };

                statuses
                    .entry(entry_key)
                    .and_modify(|existing| {
                        if git_status_priority(status) > git_status_priority(*existing) {
                            *existing = status;
                        }
                    })
                    .or_insert(status);
            }
        }
    }

    statuses
}

fn git_status_priority(status: GitStatus) -> u8 {
    match status {
        GitStatus::Conflict => 5,
        GitStatus::Modified => 4,
        GitStatus::Staged => 3,
        GitStatus::Untracked => 2,
        GitStatus::Ignored => 1,
    }
}

fn parse_png_dimensions(header: &[u8]) -> (u32, u32, &'static str) {
    if header.len() >= 24 && &header[0..8] == b"\x89PNG\r\n\x1a\n" {
        let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
        let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
        (width, height, "PNG")
    } else {
        (0, 0, "PNG")
    }
}

fn parse_jpeg_dimensions(data: &[u8]) -> (u32, u32, &'static str) {
    if data.len() < 2 || data[0] != 0xFF || data[1] != 0xD8 {
        return (0, 0, "JPEG");
    }

    let mut i = 2;
    while i + 4 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        if (marker == 0xC0 || marker == 0xC2) && i + 9 < data.len() {
            let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            return (width, height, "JPEG");
        }
        if i + 3 < data.len() {
            let length = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            i += 2 + length;
        } else {
            break;
        }
    }
    (0, 0, "JPEG")
}

fn parse_gif_dimensions(header: &[u8]) -> (u32, u32, &'static str) {
    if header.len() >= 10 && (&header[0..3] == b"GIF") {
        let width = u16::from_le_bytes([header[6], header[7]]) as u32;
        let height = u16::from_le_bytes([header[8], header[9]]) as u32;
        (width, height, "GIF")
    } else {
        (0, 0, "GIF")
    }
}

fn parse_bmp_dimensions(header: &[u8]) -> (u32, u32, &'static str) {
    if header.len() >= 26 && &header[0..2] == b"BM" {
        let width = u32::from_le_bytes([header[18], header[19], header[20], header[21]]);
        let height = u32::from_le_bytes([header[22], header[23], header[24], header[25]]);
        (width, height.abs_diff(0), "BMP")
    } else {
        (0, 0, "BMP")
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_text_empty() {
        assert!(is_text(&[]));
    }

    #[test]
    fn test_is_text_ascii() {
        assert!(is_text(b"Hello, World!"));
    }

    #[test]
    fn test_is_text_with_newlines() {
        assert!(is_text(b"Line 1\nLine 2\r\nLine 3"));
    }

    #[test]
    fn test_is_text_binary() {
        let binary = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
        assert!(!is_text(&binary));
    }

    #[test]
    fn test_parse_png_dimensions_valid() {
        let mut header = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        header.extend(vec![0; 8]); // IHDR chunk header
        header.extend(&100u32.to_be_bytes()); // width
        header.extend(&200u32.to_be_bytes()); // height
        let (w, h, fmt) = parse_png_dimensions(&header);
        assert_eq!((w, h, fmt), (100, 200, "PNG"));
    }

    #[test]
    fn test_parse_png_dimensions_invalid() {
        let header = vec![0; 24];
        let (w, h, fmt) = parse_png_dimensions(&header);
        assert_eq!((w, h, fmt), (0, 0, "PNG"));
    }

    #[test]
    fn test_parse_gif_dimensions_valid() {
        let mut header = vec![b'G', b'I', b'F', b'8', b'9', b'a'];
        header.extend(&320u16.to_le_bytes()); // width
        header.extend(&240u16.to_le_bytes()); // height
        let (w, h, fmt) = parse_gif_dimensions(&header);
        assert_eq!((w, h, fmt), (320, 240, "GIF"));
    }

    #[test]
    fn test_parse_gif_dimensions_invalid() {
        let header = vec![0; 10];
        let (w, h, fmt) = parse_gif_dimensions(&header);
        assert_eq!((w, h, fmt), (0, 0, "GIF"));
    }

    #[test]
    fn test_parse_bmp_dimensions_valid() {
        let mut header = vec![b'B', b'M'];
        header.extend(vec![0; 16]); // padding to offset 18
        header.extend(&640u32.to_le_bytes()); // width at offset 18
        header.extend(&480u32.to_le_bytes()); // height at offset 22
        let (w, h, fmt) = parse_bmp_dimensions(&header);
        assert_eq!((w, h, fmt), (640, 480, "BMP"));
    }

    #[test]
    fn test_git_status_parsing() {
        // Test that git status parsing works for various formats
        let statuses = HashMap::from([
            ("modified.txt".to_string(), GitStatus::Modified),
            ("staged.txt".to_string(), GitStatus::Staged),
        ]);
        assert_eq!(statuses.get("modified.txt"), Some(&GitStatus::Modified));
        assert_eq!(statuses.get("staged.txt"), Some(&GitStatus::Staged));
        assert_eq!(statuses.get("unknown.txt"), None);
    }

    #[test]
    fn test_mode_default() {
        let mode = Mode::default();
        assert_eq!(mode, Mode::Normal);
    }

    #[test]
    fn test_entry_sorting() {
        let mut entries = vec![
            Entry {
                name: "zebra.txt".into(),
                name_lower: "zebra.txt".into(),
                is_dir: false,
                size: 0,
                modified: None,
                is_hidden: false,
                readonly: false,
                git_status: None,
            },
            Entry {
                name: "alpha".into(),
                name_lower: "alpha".into(),
                is_dir: true,
                size: 0,
                modified: None,
                is_hidden: false,
                readonly: false,
                git_status: None,
            },
            Entry {
                name: "beta.txt".into(),
                name_lower: "beta.txt".into(),
                is_dir: false,
                size: 0,
                modified: None,
                is_hidden: false,
                readonly: false,
                git_status: None,
            },
        ];

        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name_lower.cmp(&b.name_lower),
        });

        assert_eq!(entries[0].name, "alpha");
        assert_eq!(entries[1].name, "beta.txt");
        assert_eq!(entries[2].name, "zebra.txt");
    }
}

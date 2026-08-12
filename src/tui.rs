//! Interactive worktree picker.
//!
//! The picker draws to `/dev/tty` rather than stdout, because stdout is how the
//! chosen path gets back to the shell function that performs the `cd` — it is
//! captured in a command substitution and never reaches the terminal.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{cursor, queue, style, terminal};

use crate::commands::remove::{removal_blocker, remove_worktree, RemoveOptions};
use crate::git::Worktree;
use crate::repo::Repo;

/// How the picker ended.
pub enum Outcome {
    /// Move to this worktree.
    Selected(PathBuf),
    /// Nothing was chosen; the shell should stay where it is.
    Cancelled,
}

/// `true` when a terminal is available to draw on.
pub fn is_available() -> bool {
    open_tty().is_ok()
}

fn open_tty() -> Result<File> {
    File::options()
        .write(true)
        .open("/dev/tty")
        .context("no terminal available")
}

/// Whether a command should open the picker instead of printing.
///
/// Without a terminal — a script, a pipeline, CI — the caller keeps its plain
/// behaviour. A repository whose only worktree is the main one has nothing
/// worth picking from either.
pub fn should_pick(repo: &Repo) -> Result<bool> {
    if !is_available() {
        return Ok(false);
    }
    Ok(repo.worktrees()?.len() > 1)
}

/// One row of the list.
struct Item {
    name: String,
    path: PathBuf,
    head: String,
    note: String,
    is_current: bool,
    worktree: Worktree,
}

/// List state: which rows exist, what was typed, where the cursor is.
struct Picker {
    items: Vec<Item>,
    filter: String,
    /// Index into the *filtered* list.
    cursor: usize,
    /// First visible row, for lists taller than the terminal.
    offset: usize,
}

impl Picker {
    fn new(items: Vec<Item>) -> Self {
        Self {
            items,
            filter: String::new(),
            cursor: 0,
            offset: 0,
        }
    }

    /// Indices of the items matching the filter.
    ///
    /// Matching is a case-insensitive substring test over the name and the
    /// path, which is what makes typing a fragment of a branch name work.
    fn matches(&self) -> Vec<usize> {
        if self.filter.is_empty() {
            return (0..self.items.len()).collect();
        }
        let needle = self.filter.to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.name.to_lowercase().contains(&needle)
                    || item.path.to_string_lossy().to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn selected(&self) -> Option<&Item> {
        self.matches().get(self.cursor).map(|&i| &self.items[i])
    }

    fn move_down(&mut self) {
        let len = self.matches().len();
        if len > 0 {
            self.cursor = (self.cursor + 1) % len;
        }
    }

    fn move_up(&mut self) {
        let len = self.matches().len();
        if len > 0 {
            self.cursor = (self.cursor + len - 1) % len;
        }
    }

    fn push_filter(&mut self, c: char) {
        self.filter.push(c);
        self.clamp();
    }

    fn pop_filter(&mut self) {
        self.filter.pop();
        self.clamp();
    }

    /// Keeps the cursor inside the filtered list after it shrinks or grows.
    fn clamp(&mut self) {
        let len = self.matches().len();
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }

    /// Scrolls so the cursor stays visible in a window of `height` rows.
    fn scroll_into_view(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + height {
            self.offset = self.cursor + 1 - height;
        }
    }
}

/// Restores the terminal no matter how the picker exits, including on panic.
struct Screen {
    tty: File,
}

impl Screen {
    fn open() -> Result<Self> {
        let mut tty = open_tty()?;
        terminal::enable_raw_mode().context("failed to switch the terminal to raw mode")?;
        queue!(tty, terminal::EnterAlternateScreen, cursor::Hide)?;
        tty.flush()?;
        Ok(Self { tty })
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = queue!(self.tty, cursor::Show, terminal::LeaveAlternateScreen);
        let _ = self.tty.flush();
        let _ = terminal::disable_raw_mode();
    }
}

/// Runs the picker until the user chooses a worktree or gives up.
pub fn pick(repo: &Repo) -> Result<Outcome> {
    let mut screen = Screen::open()?;
    let mut picker = Picker::new(load(repo)?);
    let mut message: Option<String> = None;

    loop {
        draw(&mut screen.tty, &mut picker, message.as_deref())?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        // Windows reports both press and release; act on press only.
        if key.kind != KeyEventKind::Press {
            continue;
        }
        message = None;

        match key_action(&key) {
            Action::Cancel => return Ok(Outcome::Cancelled),
            Action::Confirm => {
                if let Some(item) = picker.selected() {
                    return Ok(Outcome::Selected(item.path.clone()));
                }
            }
            Action::Down => picker.move_down(),
            Action::Up => picker.move_up(),
            Action::Backspace => picker.pop_filter(),
            Action::Insert(c) => picker.push_filter(c),
            Action::Delete => {
                message = delete_selected(repo, &mut screen.tty, &mut picker)?;
            }
            Action::None => {}
        }
    }
}

/// Opens the confirmation dialog and removes the worktree if confirmed.
///
/// Returns the line to show in the status area.
fn delete_selected(repo: &Repo, tty: &mut File, picker: &mut Picker) -> Result<Option<String>> {
    let Some(item) = picker.selected() else {
        return Ok(None);
    };
    let worktree = item.worktree.clone();
    let label = item.name.clone();

    if let Some(reason) = removal_blocker(repo, &worktree, true) {
        return Ok(Some(format!("cannot remove `{label}`: {reason}")));
    }
    let dirty = crate::git::is_dirty(&worktree.path).unwrap_or(false);
    let merged = worktree
        .branch
        .as_deref()
        .map(|b| crate::git::is_merged(&repo.main, b).unwrap_or(false))
        .unwrap_or(false);

    let Some(with_branch) = confirm(tty, &worktree, dirty, merged)? else {
        return Ok(None);
    };

    let opts = RemoveOptions {
        // The dialog already spelled out the risk, so honour the answer.
        force: true,
        with_branch,
        quiet: true,
    };
    match remove_worktree(repo, &worktree, opts) {
        Ok(()) => {
            picker.items = load(repo)?;
            picker.clamp();
            let extra = if with_branch { " and its branch" } else { "" };
            Ok(Some(format!("removed `{label}`{extra}")))
        }
        Err(e) => Ok(Some(format!("failed to remove `{label}`: {e}"))),
    }
}

/// Asks for confirmation. `None` means cancelled, `Some(with_branch)` confirms.
fn confirm(tty: &mut File, worktree: &Worktree, dirty: bool, merged: bool) -> Result<Option<bool>> {
    let branch = worktree.branch.clone();
    loop {
        let (_, rows) = terminal::size()?;
        queue!(
            tty,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0),
            style::Print("Remove this worktree?"),
            cursor::MoveTo(0, 2),
            style::Print(format!("  {}", worktree.path.display())),
        )?;

        let mut row = 4;
        if dirty {
            queue!(
                tty,
                cursor::MoveTo(0, row),
                style::Print("  ! uncommitted changes will be lost"),
            )?;
            row += 1;
        }
        if let Some(b) = &branch {
            let state = if merged { "merged" } else { "NOT merged" };
            queue!(
                tty,
                cursor::MoveTo(0, row),
                style::Print(format!("  branch `{b}` ({state})")),
            )?;
        }

        let keys = match &branch {
            Some(_) => "[y] remove worktree   [b] remove worktree and branch   [n] cancel",
            None => "[y] remove worktree   [n] cancel",
        };
        queue!(
            tty,
            cursor::MoveTo(0, rows.saturating_sub(1)),
            style::Print(keys)
        )?;
        tty.flush()?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(Some(false)),
            KeyCode::Char('b') | KeyCode::Char('B') if branch.is_some() => return Ok(Some(true)),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => return Ok(None),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(None),
            _ => {}
        }
    }
}

enum Action {
    Confirm,
    Cancel,
    Up,
    Down,
    Delete,
    Backspace,
    Insert(char),
    None,
}

/// Maps a key to an action.
///
/// `Delete` is bound to Ctrl-D as well: on Mac keyboards the key labelled
/// "delete" sends Backspace, which the filter needs.
fn key_action(key: &KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Enter => Action::Confirm,
        KeyCode::Esc => Action::Cancel,
        KeyCode::Char('c') | KeyCode::Char('g') if ctrl => Action::Cancel,
        KeyCode::Down => Action::Down,
        KeyCode::Up => Action::Up,
        KeyCode::Char('n') if ctrl => Action::Down,
        KeyCode::Char('p') if ctrl => Action::Up,
        KeyCode::Char('d') if ctrl => Action::Delete,
        KeyCode::Delete => Action::Delete,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Char(c) if !ctrl => Action::Insert(c),
        _ => Action::None,
    }
}

fn load(repo: &Repo) -> Result<Vec<Item>> {
    let worktrees = repo.worktrees()?;
    let Some(main) = worktrees.first().cloned() else {
        return Ok(Vec::new());
    };
    Ok(worktrees
        .into_iter()
        .map(|wt| Item {
            name: repo.display_name(&wt, &main),
            head: wt.short_head(),
            note: note(&wt),
            is_current: repo.cwd.starts_with(&wt.path),
            path: wt.path.clone(),
            worktree: wt,
        })
        .collect())
}

fn note(wt: &Worktree) -> String {
    let mut notes = Vec::new();
    if wt.bare {
        notes.push("bare");
    }
    if wt.detached {
        notes.push("detached");
    }
    if wt.locked {
        notes.push("locked");
    }
    if notes.is_empty() {
        String::new()
    } else {
        format!(" ({})", notes.join(", "))
    }
}

/// Kept to plain ASCII on purpose.
///
/// Arrows and the return symbol land in ranges that terminals disagree about:
/// some fonts have no glyph, and U+25B6 in particular has an emoji
/// presentation that renders double width and shifts the whole row.
const HELP: &str = "up/down move   enter cd   ctrl-d delete   esc cancel";

/// Truncates to `width` and pads to it, so a highlighted row spans the line.
fn fit(line: &str, width: usize) -> String {
    let mut out: String = line.chars().take(width).collect();
    let len = out.chars().count();
    if len < width {
        out.extend(std::iter::repeat_n(' ', width - len));
    }
    out
}

fn draw(tty: &mut File, picker: &mut Picker, message: Option<&str>) -> Result<()> {
    let (cols, rows) = terminal::size()?;
    let cols = cols as usize;
    // One line for the prompt, one for help, one for any message.
    let reserved = if message.is_some() { 3 } else { 2 };
    let height = (rows as usize).saturating_sub(reserved);
    picker.scroll_into_view(height);

    let matches = picker.matches();
    let name_width = matches
        .iter()
        .map(|&i| picker.items[i].name.chars().count())
        .max()
        .unwrap_or(0);

    queue!(
        tty,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0),
        style::Print(format!("> {}", picker.filter)),
    )?;

    for (row, &index) in matches.iter().skip(picker.offset).take(height).enumerate() {
        let item = &picker.items[index];
        let is_cursor = picker.offset + row == picker.cursor;
        // The marker column says "you are standing here"; the cursor is the
        // highlight, so the two never compete for the same character.
        let marker = if item.is_current { "*" } else { " " };
        let line = format!(
            "{marker} {:<name_width$}  {}  {}{}",
            item.name,
            item.head,
            item.path.display(),
            item.note
        );
        let line = fit(&line, cols);
        queue!(tty, cursor::MoveTo(0, row as u16 + 1))?;
        if is_cursor {
            // Reverse video rather than a colour: it stays readable on any
            // theme, and highlights the row edge to edge.
            queue!(
                tty,
                style::SetAttribute(style::Attribute::Reverse),
                style::Print(line),
                style::SetAttribute(style::Attribute::Reset),
            )?;
        } else {
            queue!(tty, style::Print(line))?;
        }
    }

    if let Some(message) = message {
        queue!(
            tty,
            cursor::MoveTo(0, rows.saturating_sub(2)),
            style::Print(message.chars().take(cols).collect::<String>()),
        )?;
    }
    queue!(
        tty,
        cursor::MoveTo(0, rows.saturating_sub(1)),
        style::Print(HELP),
    )?;
    tty.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, path: &str) -> Item {
        Item {
            name: name.to_string(),
            path: PathBuf::from(path),
            head: "abc1234".to_string(),
            note: String::new(),
            is_current: false,
            worktree: Worktree {
                path: PathBuf::from(path),
                head: None,
                branch: Some(name.to_string()),
                bare: false,
                detached: false,
                locked: false,
            },
        }
    }

    fn picker() -> Picker {
        Picker::new(vec![
            item("@", "/repo"),
            item("feature/auth", "/wt/feature/auth"),
            item("feature/billing", "/wt/feature/billing"),
            item("hotfix", "/wt/hotfix"),
        ])
    }

    #[test]
    fn the_interface_is_ascii_only() {
        // Non-ASCII risks missing glyphs, and emoji-presentation characters
        // render double width and break the column alignment.
        assert!(HELP.is_ascii(), "{HELP}");
        for label in [
            "Remove this worktree?",
            "  ! uncommitted changes will be lost",
            "[y] remove worktree   [b] remove worktree and branch   [n] cancel",
        ] {
            assert!(label.is_ascii(), "{label}");
        }
    }

    #[test]
    fn the_help_spells_out_the_modifier_key() {
        // `^d` reads as noise to anyone who has not seen the convention.
        assert!(HELP.contains("ctrl-d"), "{HELP}");
        assert!(!HELP.contains("^d"), "{HELP}");
    }

    #[test]
    fn rows_are_padded_so_a_highlight_covers_the_line() {
        assert_eq!(fit("abc", 6), "abc   ");
        assert_eq!(fit("abcdefgh", 4), "abcd");
        assert_eq!(fit("", 3), "   ");
    }

    #[test]
    fn filter_matches_name_and_path_case_insensitively() {
        let mut p = picker();
        p.filter = "AUTH".to_string();
        assert_eq!(p.matches().len(), 1);
        assert_eq!(p.selected().unwrap().name, "feature/auth");

        p.filter = "feature".to_string();
        assert_eq!(p.matches().len(), 2);

        p.filter = "/wt/hot".to_string();
        assert_eq!(p.selected().unwrap().name, "hotfix");
    }

    #[test]
    fn cursor_wraps_around() {
        let mut p = picker();
        p.move_up();
        assert_eq!(p.selected().unwrap().name, "hotfix");
        p.move_down();
        assert_eq!(p.selected().unwrap().name, "@");
    }

    #[test]
    fn cursor_stays_inside_a_shrinking_list() {
        let mut p = picker();
        p.cursor = 3;
        for c in "feature".chars() {
            p.push_filter(c);
        }
        // Only the two `feature/*` rows remain, so the cursor cannot stay at 3.
        assert_eq!(p.matches().len(), 2);
        assert_eq!(p.cursor, 1);
        assert_eq!(p.selected().unwrap().name, "feature/billing");
    }

    #[test]
    fn no_match_leaves_nothing_selected() {
        let mut p = picker();
        p.filter = "zzz".to_string();
        p.clamp();
        assert!(p.selected().is_none());
    }

    #[test]
    fn backspace_restores_matches() {
        let mut p = picker();
        p.push_filter('z');
        assert!(p.selected().is_none());
        p.pop_filter();
        assert_eq!(p.matches().len(), 4);
    }

    #[test]
    fn scrolling_follows_the_cursor() {
        let mut p = picker();
        p.cursor = 3;
        p.scroll_into_view(2);
        assert_eq!(p.offset, 2);
        p.cursor = 0;
        p.scroll_into_view(2);
        assert_eq!(p.offset, 0);
    }

    #[test]
    fn delete_is_bound_to_both_delete_and_ctrl_d() {
        let del = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
        let ctrl_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert!(matches!(key_action(&del), Action::Delete));
        assert!(matches!(key_action(&ctrl_d), Action::Delete));
        // Backspace must stay with the filter, not delete a worktree.
        let backspace = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        assert!(matches!(key_action(&backspace), Action::Backspace));
    }

    #[test]
    fn plain_characters_type_into_the_filter() {
        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(matches!(key_action(&a), Action::Insert('a')));
        // Ctrl-n/p navigate instead of typing.
        let ctrl_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL);
        assert!(matches!(key_action(&ctrl_n), Action::Down));
    }
}

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
use crate::git::{self, Worktree};
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
    /// Empty until [`StatusFeed`] reports back.
    note: String,
    is_current: bool,
    is_main: bool,
    worktree: Worktree,
}

/// What a Backspace did, since the key has two jobs.
#[derive(Debug, PartialEq, Eq)]
enum Backspace {
    /// Removed a character from the filter.
    ErasedFilter,
    /// Swallowed, because the filter had only just been emptied.
    Absorbed,
    /// Asked to remove the selected worktree.
    Delete,
}

/// List state: which rows exist, what was typed, where the cursor is.
struct Picker {
    items: Vec<Item>,
    filter: String,
    /// Index into the *filtered* list.
    cursor: usize,
    /// First visible row, for lists taller than the terminal.
    offset: usize,
    /// Set while Backspace is being used to erase the filter.
    ///
    /// Holding the key to clear what you typed sends a burst of Backspaces;
    /// without this, the burst would run past the empty filter and open the
    /// delete dialog. One press is swallowed at the boundary, so reaching the
    /// dialog always takes a deliberate keystroke.
    erasing: bool,
}

impl Picker {
    fn new(items: Vec<Item>) -> Self {
        Self {
            items,
            filter: String::new(),
            cursor: 0,
            offset: 0,
            erasing: false,
        }
    }

    /// Resolves what Backspace means right now.
    fn backspace(&mut self) -> Backspace {
        if !self.filter.is_empty() {
            self.pop_filter();
            self.erasing = true;
            return Backspace::ErasedFilter;
        }
        if self.erasing {
            self.erasing = false;
            return Backspace::Absorbed;
        }
        Backspace::Delete
    }

    /// Any other key ends the erasing streak.
    fn note_other_key(&mut self) {
        self.erasing = false;
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

/// How often the loop looks for status results while they are still coming.
const STATUS_POLL: std::time::Duration = std::time::Duration::from_millis(60);

/// Runs the picker until the user chooses a worktree or gives up.
pub fn pick(repo: &Repo) -> Result<Outcome> {
    let mut screen = Screen::open()?;
    let mut picker = Picker::new(load(repo)?);
    let mut status = StatusFeed::spawn(repo, &picker.items);
    let mut message: Option<String> = None;

    loop {
        draw(&mut screen.tty, &mut picker, message.as_deref())?;

        let Some(key) = next_key(&mut picker, &mut status)? else {
            // Status arrived rather than a key; show it.
            continue;
        };
        // Windows reports both press and release; act on press only.
        if key.kind != KeyEventKind::Press {
            continue;
        }
        message = None;

        let action = key_action(&key);
        if action != Action::Backspace {
            picker.note_other_key();
        }

        match action {
            Action::Cancel => return Ok(Outcome::Cancelled),
            Action::Confirm => {
                if let Some(item) = picker.selected() {
                    return Ok(Outcome::Selected(item.path.clone()));
                }
            }
            Action::Down => picker.move_down(),
            Action::Up => picker.move_up(),
            Action::Backspace => {
                if picker.backspace() == Backspace::Delete {
                    message = delete_selected(repo, &mut screen.tty, &mut picker)?;
                    status = StatusFeed::spawn(repo, &picker.items);
                }
            }
            Action::Insert(c) => picker.push_filter(c),
            Action::Delete => {
                message = delete_selected(repo, &mut screen.tty, &mut picker)?;
                status = StatusFeed::spawn(repo, &picker.items);
            }
            Action::None => {}
        }
    }
}

/// Waits for a key, redrawing when the status column fills in first.
///
/// Returns `None` when something other than a key needs the screen redrawn.
fn next_key(picker: &mut Picker, status: &mut StatusFeed) -> Result<Option<KeyEvent>> {
    loop {
        if status.is_done() {
            // Nothing left to wait for, so stop waking up to check.
            return Ok(match event::read()? {
                Event::Key(key) => Some(key),
                _ => None,
            });
        }
        if event::poll(STATUS_POLL)? {
            return Ok(match event::read()? {
                Event::Key(key) => Some(key),
                _ => None,
            });
        }
        if status.drain(&mut picker.items) {
            return Ok(None);
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

    // Removing can take a moment. Say so before starting, or the dialog just
    // sits there after the answer and invites a second, harder press.
    working(tty, &format!("Removing {}...", worktree.path.display()))?;

    let opts = RemoveOptions {
        // The dialog already spelled out the risk, so honour the answer.
        force: true,
        with_branch,
        quiet: true,
    };
    let outcome = match remove_worktree(repo, &worktree, opts) {
        Ok(()) => {
            picker.items = load(repo)?;
            picker.clamp();
            let extra = if with_branch { " and its branch" } else { "" };
            Some(format!("removed `{label}`{extra}"))
        }
        Err(e) => Some(format!("failed to remove `{label}`: {e}")),
    };

    // Anything typed while git was working was meant for the dialog, not for
    // the list that is about to replace it.
    discard_pending_input()?;
    Ok(outcome)
}

/// Clears the screen and states what is happening, for work that blocks.
fn working(tty: &mut File, what: &str) -> Result<()> {
    let (_, rows) = terminal::size()?;
    queue!(
        tty,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0),
        style::Print(what),
        cursor::MoveTo(0, rows.saturating_sub(1)),
    )?;
    tty.flush()?;
    Ok(())
}

fn discard_pending_input() -> Result<()> {
    while event::poll(std::time::Duration::from_millis(0))? {
        let _ = event::read()?;
    }
    Ok(())
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

#[derive(Debug, PartialEq, Eq)]
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
        // Terminals told to send 0x08 for the erase key surface it as Ctrl-H.
        KeyCode::Char('h') if ctrl => Action::Backspace,
        KeyCode::Char(c) if !ctrl => Action::Insert(c),
        _ => Action::None,
    }
}

/// Builds the rows without asking git anything slow.
///
/// The status column is left empty here and filled in by [`StatusFeed`]: it
/// costs a `git status` per worktree, which is fast in a small repository and
/// seconds across ten of them. The list has to be on screen before that.
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
            note: String::new(),
            is_main: wt.path == main.path,
            is_current: repo.cwd.starts_with(&wt.path),
            path: wt.path.clone(),
            worktree: wt,
        })
        .collect())
}

/// Fills in the status column without blocking the list.
///
/// One background thread asks which branches are merged, then fans out a
/// thread per worktree for the `git status` calls, so ten worktrees cost about
/// as long as the slowest one rather than the sum of all ten.
struct StatusFeed {
    rx: Option<std::sync::mpsc::Receiver<Vec<(usize, String)>>>,
}

/// Everything a worker needs to describe one worktree, owned so it can move
/// to another thread.
struct Subject {
    index: usize,
    path: PathBuf,
    branch: Option<String>,
    is_main: bool,
    flags: Vec<&'static str>,
}

impl StatusFeed {
    fn spawn(repo: &Repo, items: &[Item]) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let main = repo.main.clone();
        let subjects: Vec<Subject> = items
            .iter()
            .enumerate()
            .map(|(index, item)| Subject {
                index,
                path: item.path.clone(),
                branch: item.worktree.branch.clone(),
                is_main: item.is_main,
                flags: flags(&item.worktree),
            })
            .collect();

        std::thread::spawn(move || {
            let merged = git::merged_branches(&main).unwrap_or_default();
            let mut workers = Vec::new();
            for subject in subjects {
                let merged = merged.clone();
                workers.push(std::thread::spawn(move || {
                    let note = note(
                        &subject.path,
                        subject.branch.as_deref(),
                        &merged,
                        subject.is_main,
                        &subject.flags,
                    );
                    (subject.index, note)
                }));
            }
            let notes = workers.into_iter().filter_map(|w| w.join().ok()).collect();
            // The receiver is gone once the picker closes; nothing to report.
            let _ = tx.send(notes);
        });

        Self { rx: Some(rx) }
    }

    /// Applies whatever has arrived. `true` when the screen needs redrawing.
    fn drain(&mut self, items: &mut [Item]) -> bool {
        use std::sync::mpsc::TryRecvError;
        let Some(rx) = &self.rx else {
            return false;
        };
        match rx.try_recv() {
            Ok(notes) => {
                for (index, note) in notes {
                    if let Some(item) = items.get_mut(index) {
                        item.note = note;
                    }
                }
                self.rx = None;
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                false
            }
        }
    }

    /// Once nothing more is coming, the loop can go back to blocking on keys.
    fn is_done(&self) -> bool {
        self.rx.is_none()
    }
}

/// What is worth knowing about a worktree before acting on it.
///
/// The path used to sit here, but it is derived from the branch name for every
/// worktree gwt creates, so it repeated what the name already said. What is
/// missing at a glance is whether removing this one would lose anything.
///
/// "merged" is skipped for the main worktree: a branch is always merged into
/// itself, and the main worktree cannot be removed anyway.
fn note(
    path: &std::path::Path,
    branch: Option<&str>,
    merged: &[String],
    is_main: bool,
    flags: &[&str],
) -> String {
    let mut notes: Vec<String> = Vec::new();

    if git::is_dirty(path).unwrap_or(false) {
        notes.push("dirty".to_string());
    }
    if !is_main {
        if let Some(branch) = branch {
            if merged.iter().any(|b| b == branch) {
                notes.push("merged".to_string());
            }
        }
    }
    notes.extend(flags.iter().map(|f| f.to_string()));
    notes.join(", ")
}

fn flags(wt: &Worktree) -> Vec<&'static str> {
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
    notes
}

/// One entry of the help line: the key, then what it does.
///
/// The key is drawn in reverse video, which gives the boundary between key and
/// label a shape rather than leaving it to whitespace. It reuses the attribute
/// the cursor row already uses, so it holds up on any colour theme.
struct Hint {
    keys: &'static [&'static str],
    label: &'static str,
    /// Dropped first when the terminal is too narrow for the full line.
    optional: bool,
}

/// Keys are spelled out: `bksp` is keycap shorthand, not something a help line
/// can assume its reader knows.
///
/// Everything here is plain ASCII on purpose — arrows and the return symbol
/// land in ranges that terminals disagree about, and U+25B6 in particular has
/// an emoji presentation that renders double width and shifts the whole row.
const HINTS_IDLE: &[Hint] = &[
    Hint {
        keys: &["up/down"],
        label: "move",
        optional: true,
    },
    Hint {
        keys: &["enter"],
        label: "cd",
        optional: false,
    },
    Hint {
        keys: &["ctrl-d", "backspace"],
        label: "delete",
        optional: false,
    },
    Hint {
        keys: &["esc"],
        label: "cancel",
        optional: false,
    },
];

/// While a filter is typed, Backspace edits it instead of deleting.
const HINTS_FILTERING: &[Hint] = &[
    Hint {
        keys: &["up/down"],
        label: "move",
        optional: true,
    },
    Hint {
        keys: &["enter"],
        label: "cd",
        optional: false,
    },
    Hint {
        keys: &["ctrl-d"],
        label: "delete",
        optional: false,
    },
    Hint {
        keys: &["backspace"],
        label: "erase",
        optional: false,
    },
    Hint {
        keys: &["esc"],
        label: "cancel",
        optional: false,
    },
];

/// The hints matching what Backspace does right now.
fn hints_for(picker: &Picker) -> &'static [Hint] {
    if picker.filter.is_empty() {
        HINTS_IDLE
    } else {
        HINTS_FILTERING
    }
}

/// Columns a set of hints needs, badges included.
fn hints_width(hints: &[&Hint]) -> usize {
    hints
        .iter()
        .map(|h| {
            // Each key sits in a badge padded by one space on either side.
            let keys: usize = h.keys.iter().map(|k| k.chars().count() + 2).sum();
            let between_keys = h.keys.len() - 1;
            keys + between_keys + 1 + h.label.chars().count()
        })
        .sum::<usize>()
        + hints.len().saturating_sub(1) * 2
}

/// Drops optional hints until the line fits, rather than truncating mid-word.
fn hints_that_fit(hints: &'static [Hint], width: usize) -> Vec<&'static Hint> {
    let mut kept: Vec<&Hint> = hints.iter().collect();
    while hints_width(&kept) > width && kept.iter().any(|h| h.optional) {
        let index = kept.iter().position(|h| h.optional).unwrap();
        kept.remove(index);
    }
    kept
}

/// Truncates to `width` and pads to it, so a highlighted row spans the line.
fn fit(line: &str, width: usize) -> String {
    let mut out: String = line.chars().take(width).collect();
    let len = out.chars().count();
    if len < width {
        out.extend(std::iter::repeat_n(' ', width - len));
    }
    out
}

const PLACEHOLDER: &str = "type to filter";

/// Column labels. `HEAD` keeps git's own name for the short commit id.
const NAME_HEADER: &str = "WORKTREE";
const HEAD_HEADER: &str = "HEAD   ";
const STATUS_HEADER: &str = "STATUS";

/// What the right of the prompt says about the list below it.
///
/// While filtering, this is the only place the size of the list is stated —
/// without it, typing something that matches nothing just empties the screen
/// with no explanation.
fn count_label(filter: &str, matched: usize, total: usize) -> String {
    if !filter.is_empty() {
        return format!("{matched} of {total}");
    }
    match total {
        1 => "1 worktree".to_string(),
        n => format!("{n} worktrees"),
    }
}

/// Draws the filter line: prompt, what has been typed, a block cursor, and —
/// while nothing is typed — what typing would do.
///
/// The terminal's own cursor is hidden for the whole picker, so the block is
/// drawn by hand as a space in reverse video. Without it, an empty filter line
/// is a lone `>` that gives no sign it accepts input.
fn draw_prompt(tty: &mut File, picker: &Picker, matched: usize, cols: usize) -> Result<()> {
    let placeholder = if picker.filter.is_empty() {
        PLACEHOLDER
    } else {
        ""
    };
    let count = count_label(&picker.filter, matched, picker.items.len());
    let left = 2 + picker.filter.chars().count() + 1 + placeholder.chars().count();

    queue!(tty, style::Print("> "), style::Print(&picker.filter))?;
    queue!(
        tty,
        style::SetAttribute(style::Attribute::Reverse),
        style::Print(" "),
        style::SetAttribute(style::Attribute::Reset),
    )?;
    if !placeholder.is_empty() {
        queue!(
            tty,
            style::SetAttribute(style::Attribute::Dim),
            style::Print(placeholder),
            style::SetAttribute(style::Attribute::Reset),
        )?;
    }

    // Right-aligned, and dropped rather than wrapped when the line is full.
    let gap = cols.saturating_sub(left + count.chars().count());
    if gap > 0 {
        queue!(
            tty,
            style::Print(" ".repeat(gap)),
            style::SetAttribute(style::Attribute::Dim),
            style::Print(count),
            style::SetAttribute(style::Attribute::Reset),
        )?;
    }
    Ok(())
}

fn draw(tty: &mut File, picker: &mut Picker, message: Option<&str>) -> Result<()> {
    let (cols, rows) = terminal::size()?;
    let cols = cols as usize;
    // The prompt, the header, the help line, and any message.
    let reserved = if message.is_some() { 4 } else { 3 };
    let height = (rows as usize).saturating_sub(reserved);
    picker.scroll_into_view(height);

    let matches = picker.matches();
    let name_width = matches
        .iter()
        .map(|&i| picker.items[i].name.chars().count())
        .max()
        .unwrap_or(0)
        // Never narrower than the column it is labelled with.
        .max(NAME_HEADER.len());

    // The table starts at the top, so the header sits against the rows it
    // labels; the filter joins the help line at the bottom, where the things
    // you operate live. Butting the filter against the header, as this used
    // to, made the two read as one block of chrome.
    //
    // Bold and underlined, not dimmed. Dim is what the filter placeholder
    // uses, and a header that shares it reads as another piece of hint text
    // instead of the structure of the table. The underline runs the full width
    // so it doubles as the rule between the header and the rows; carrying both
    // attributes means the header still stands out on a terminal that renders
    // only one of them.
    let header = fit(
        &format!("  {NAME_HEADER:<name_width$}  {HEAD_HEADER}  {STATUS_HEADER}"),
        cols,
    );
    queue!(
        tty,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0),
        style::SetAttribute(style::Attribute::Bold),
        style::SetAttribute(style::Attribute::Underlined),
        style::Print(header),
        style::SetAttribute(style::Attribute::Reset),
    )?;

    for (row, &index) in matches.iter().skip(picker.offset).take(height).enumerate() {
        let item = &picker.items[index];
        let is_cursor = picker.offset + row == picker.cursor;
        // The marker column says "you are standing here"; the cursor is the
        // highlight, so the two never compete for the same character.
        let marker = if item.is_current { "*" } else { " " };
        let line = format!(
            "{marker} {:<name_width$}  {}  {}",
            item.name, item.head, item.note
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
            cursor::MoveTo(0, rows.saturating_sub(3)),
            style::Print(message.chars().take(cols).collect::<String>()),
        )?;
    }
    queue!(tty, cursor::MoveTo(0, rows.saturating_sub(2)))?;
    draw_prompt(tty, picker, matches.len(), cols)?;

    queue!(tty, cursor::MoveTo(0, rows.saturating_sub(1)))?;
    draw_hints(tty, &hints_that_fit(hints_for(picker), cols))?;
    tty.flush()?;
    Ok(())
}

/// Draws the help line, each key as a reverse-video badge.
fn draw_hints(tty: &mut File, hints: &[&Hint]) -> Result<()> {
    for (i, hint) in hints.iter().enumerate() {
        if i > 0 {
            queue!(tty, style::Print("  "))?;
        }
        for (k, key) in hint.keys.iter().enumerate() {
            if k > 0 {
                queue!(tty, style::Print(" "))?;
            }
            queue!(
                tty,
                style::SetAttribute(style::Attribute::Reverse),
                style::Print(format!(" {key} ")),
                style::SetAttribute(style::Attribute::Reset),
            )?;
        }
        queue!(tty, style::Print(format!(" {}", hint.label)))?;
    }
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
            is_main: false,
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
        for hint in HINTS_IDLE.iter().chain(HINTS_FILTERING) {
            for key in hint.keys {
                assert!(key.is_ascii(), "{key}");
            }
            assert!(hint.label.is_ascii(), "{}", hint.label);
        }
        for label in [
            "Remove this worktree?",
            "  ! uncommitted changes will be lost",
            "[y] remove worktree   [b] remove worktree and branch   [n] cancel",
        ] {
            assert!(label.is_ascii(), "{label}");
        }
    }

    #[test]
    fn keys_are_spelled_out() {
        // `^d` and `bksp` are shorthand a help line cannot assume its reader
        // knows, so both are written in full.
        let keys: Vec<&str> = HINTS_IDLE
            .iter()
            .chain(HINTS_FILTERING)
            .flat_map(|h| h.keys.iter().copied())
            .collect();
        assert!(keys.contains(&"ctrl-d"), "{keys:?}");
        assert!(keys.contains(&"backspace"), "{keys:?}");
        assert!(!keys.iter().any(|k| k.contains("bksp") || k.contains('^')));
    }

    #[test]
    fn the_help_says_what_backspace_will_do() {
        let mut p = picker();
        let idle = hints_for(&p);
        let delete = idle.iter().find(|h| h.label == "delete").unwrap();
        assert!(delete.keys.contains(&"backspace"), "{:?}", delete.keys);

        p.push_filter('a');
        let filtering = hints_for(&p);
        let erase = filtering.iter().find(|h| h.label == "erase").unwrap();
        assert_eq!(erase.keys, &["backspace"]);
        // While filtering, deleting is ctrl-d only.
        let delete = filtering.iter().find(|h| h.label == "delete").unwrap();
        assert_eq!(delete.keys, &["ctrl-d"]);
    }

    #[test]
    fn a_narrow_terminal_drops_optional_hints_instead_of_cutting_words() {
        let full = hints_width(&HINTS_IDLE.iter().collect::<Vec<_>>());
        assert_eq!(hints_that_fit(HINTS_IDLE, full).len(), HINTS_IDLE.len());

        // One column short: the optional "move" hint goes, the rest survive.
        let narrowed = hints_that_fit(HINTS_IDLE, full - 1);
        assert_eq!(narrowed.len(), HINTS_IDLE.len() - 1);
        assert!(!narrowed.iter().any(|h| h.label == "move"));
        assert!(narrowed.iter().any(|h| h.label == "cancel"));

        // Nothing optional left to drop: the required hints are kept.
        assert!(!hints_that_fit(HINTS_IDLE, 1).is_empty());
    }

    #[test]
    fn the_count_explains_an_empty_list() {
        // Nothing typed: the size of the list.
        assert_eq!(count_label("", 4, 4), "4 worktrees");
        assert_eq!(count_label("", 1, 1), "1 worktree");
        // Typing: how much of it survived, so filtering everything away reads
        // as `0 of 4` instead of an unexplained blank screen.
        assert_eq!(count_label("bill", 1, 4), "1 of 4");
        assert_eq!(count_label("zzz", 0, 4), "0 of 4");
    }

    #[test]
    fn the_column_headers_are_ascii() {
        for header in [NAME_HEADER, HEAD_HEADER, STATUS_HEADER] {
            assert!(header.is_ascii(), "{header}");
        }
    }

    #[test]
    fn rows_start_without_a_status() {
        // The list is drawn before git is asked anything slow, so the status
        // column starts empty and fills in later.
        let p = picker();
        assert!(p.items.iter().all(|i| i.note.is_empty()));
    }

    #[test]
    fn the_placeholder_is_ascii_and_says_what_typing_does() {
        assert!(PLACEHOLDER.is_ascii(), "{PLACEHOLDER}");
        assert!(PLACEHOLDER.contains("filter"), "{PLACEHOLDER}");
    }

    #[test]
    fn backspace_removes_a_worktree_when_nothing_is_typed() {
        let mut p = picker();
        assert_eq!(p.backspace(), Backspace::Delete);
    }

    #[test]
    fn backspace_edits_the_filter_while_there_is_one() {
        let mut p = picker();
        p.push_filter('a');
        p.push_filter('u');
        assert_eq!(p.backspace(), Backspace::ErasedFilter);
        assert_eq!(p.backspace(), Backspace::ErasedFilter);
        assert_eq!(p.filter, "");
    }

    #[test]
    fn holding_backspace_to_clear_cannot_run_into_a_deletion() {
        let mut p = picker();
        for c in "auth".chars() {
            p.push_filter(c);
        }
        // The burst that empties the filter.
        for _ in 0..4 {
            assert_eq!(p.backspace(), Backspace::ErasedFilter);
        }
        // The overshoot from key repeat is swallowed instead of deleting.
        assert_eq!(p.backspace(), Backspace::Absorbed);
        // A deliberate press after that still works.
        assert_eq!(p.backspace(), Backspace::Delete);
    }

    #[test]
    fn any_other_key_ends_the_erasing_streak() {
        let mut p = picker();
        p.push_filter('a');
        assert_eq!(p.backspace(), Backspace::ErasedFilter);
        // Moving the cursor means the next Backspace is a fresh intent.
        p.note_other_key();
        assert_eq!(p.backspace(), Backspace::Delete);
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

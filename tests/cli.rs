//! End-to-end tests driving the real binary against real git repositories.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_gwx");

/// Starts a command that cannot reach outside its fixture.
///
/// git reads its own environment before it reads `--git-dir` or the working
/// directory, so a stray `GIT_DIR` sends every call in these tests at whatever
/// repository set it. That is not hypothetical: run `cargo test` from a git
/// hook — which is exactly what `.githooks/pre-commit` does — and git has
/// already exported `GIT_DIR` pointing at the real checkout. The fixtures then
/// reconfigure *it*, and `git init --bare` turns the working repository bare.
fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_COMMON_DIR",
        "GIT_PREFIX",
        "GIT_CONFIG",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
}

impl Fixture {
    /// A repository with one commit on `main`, plus a bare `origin` remote.
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        // macOS hands out /var/... symlinks; git reports the resolved path.
        let root = tmp.path().canonicalize().unwrap();
        let repo = root.join("repo");
        let origin = root.join("origin.git");

        git(
            &root,
            ["init", "--bare", "--initial-branch=main", "origin.git"],
        );
        git(&root, ["init", "--initial-branch=main", "repo"]);
        git(&repo, ["config", "user.email", "test@example.com"]);
        git(&repo, ["config", "user.name", "Test"]);
        git(&repo, ["remote", "add", "origin", origin.to_str().unwrap()]);
        std::fs::write(repo.join("README.md"), "hello\n").unwrap();
        git(&repo, ["add", "."]);
        git(&repo, ["commit", "-m", "init"]);
        git(&repo, ["push", "-u", "origin", "main"]);

        Self {
            _tmp: tmp,
            root,
            repo,
        }
    }

    fn worktrees_dir(&self) -> PathBuf {
        self.root.join("worktrees")
    }

    /// Adds a second bare remote, for the cases where a branch name is not
    /// unique across remotes.
    fn add_remote(&self, name: &str) {
        let path = self.root.join(format!("{name}.git"));
        git(
            &self.root,
            [
                "init",
                "--bare",
                "--initial-branch=main",
                &format!("{name}.git"),
            ],
        );
        git(&self.repo, ["remote", "add", name, path.to_str().unwrap()]);
    }

    fn gwx<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.gwx_in(&self.repo, args)
    }

    fn gwx_in<I, S>(&self, dir: &Path, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        command(BIN)
            .current_dir(dir)
            .args(args)
            .output()
            .expect("failed to run gwx")
    }

    fn gwx_ok<I, S>(&self, args: I) -> String
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let out = self.gwx(args);
        assert!(
            out.status.success(),
            "gwx failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim_end().to_string()
    }

    fn write_config(&self, contents: &str) {
        std::fs::write(self.repo.join(".gwx.toml"), contents).unwrap();
    }

    /// Asks for the candidates a shell would get for `words[index]`.
    ///
    /// This is the same protocol the scripts from `gwx completion` speak.
    fn complete_in(&self, dir: &Path, index: usize, words: &[&str]) -> Vec<String> {
        let out = command(BIN)
            .current_dir(dir)
            .env("COMPLETE", "bash")
            .env("_CLAP_COMPLETE_INDEX", index.to_string())
            .arg("--")
            .args(words)
            .output()
            .expect("failed to run gwx");
        assert!(
            out.status.success(),
            "completion failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Candidates for the last (empty) word of `words`.
    fn complete(&self, words: &[&str]) -> Vec<String> {
        self.complete_in(&self.repo, words.len() - 1, words)
    }
}

fn git<I, S>(dir: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let out = command("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to run git");
    assert!(
        out.status.success(),
        "git failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_out<I, S>(dir: &Path, args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let out = command("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to run git");
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

#[test]
fn add_creates_a_branch_and_a_worktree() {
    let fx = Fixture::new();
    let path = fx.gwx_ok(["add", "feature/auth"]);

    let expected = fx.worktrees_dir().join("feature/auth");
    assert_eq!(Path::new(&path), expected);
    assert!(expected.join("README.md").exists());
    assert_eq!(
        git_out(&expected, ["rev-parse", "--abbrev-ref", "HEAD"]),
        "feature/auth"
    );
}

#[test]
fn add_checks_out_an_existing_branch() {
    let fx = Fixture::new();
    git(&fx.repo, ["branch", "existing"]);

    let path = fx.gwx_ok(["add", "existing"]);
    assert_eq!(
        git_out(Path::new(&path), ["rev-parse", "--abbrev-ref", "HEAD"]),
        "existing"
    );
}

#[test]
fn add_tracks_a_remote_only_branch() {
    let fx = Fixture::new();
    git(&fx.repo, ["branch", "remote-only"]);
    git(&fx.repo, ["push", "origin", "remote-only"]);
    git(&fx.repo, ["branch", "-D", "remote-only"]);

    let path = fx.gwx_ok(["add", "remote-only"]);
    let upstream = git_out(
        Path::new(&path),
        ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    );
    assert_eq!(upstream, "origin/remote-only");
}

#[test]
fn add_respects_from_and_path() {
    let fx = Fixture::new();
    let custom = fx.root.join("custom-dir");
    let path = fx.gwx_ok([
        "add",
        "from-main",
        "--from",
        "main",
        "--path",
        custom.to_str().unwrap(),
    ]);

    assert_eq!(Path::new(&path), custom);
    assert_eq!(
        git_out(&custom, ["rev-parse", "HEAD"]),
        git_out(&fx.repo, ["rev-parse", "main"])
    );
}

#[test]
fn add_refuses_a_branch_that_is_already_checked_out() {
    let fx = Fixture::new();
    fx.gwx_ok(["add", "dup"]);

    let out = fx.gwx(["add", "dup"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("already checked out"), "{stderr}");
}

#[test]
fn no_create_fails_for_unknown_branches() {
    let fx = Fixture::new();
    let out = fx.gwx(["add", "nope", "--no-create"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("does not exist"));
}

#[test]
fn base_dir_is_configurable() {
    let fx = Fixture::new();
    fx.write_config("[defaults]\nbase_dir = \"../trees\"\n");

    let path = fx.gwx_ok(["add", "feat"]);
    assert_eq!(Path::new(&path), fx.root.join("trees/feat"));
}

#[test]
fn hooks_run_before_and_after_creation() {
    let fx = Fixture::new();
    std::fs::write(fx.repo.join(".env"), "SECRET=1\n").unwrap();
    fx.write_config(
        r#"
version = "1"

[[hooks.pre_create]]
type = "command"
command = "printf %s \"$GWX_BRANCH\" > pre-marker"

[[hooks.post_create]]
type = "copy"
from = ".env"

[[hooks.post_create]]
type = "symlink"
from = "README.md"
to = "linked-readme"

[[hooks.post_create]]
type = "command"
command = "printf %s \"$GWX_WORKTREE_PATH:$MY_VAR\" > post-marker"
env = { MY_VAR = "set" }
"#,
    );

    let path = PathBuf::from(fx.gwx_ok(["add", "feature/hooked"]));

    // pre_create runs in the main worktree, before the new one exists.
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("pre-marker")).unwrap(),
        "feature/hooked"
    );
    assert_eq!(
        std::fs::read_to_string(path.join(".env")).unwrap(),
        "SECRET=1\n"
    );
    assert!(path
        .join("linked-readme")
        .symlink_metadata()
        .unwrap()
        .is_symlink());
    assert_eq!(
        std::fs::read_to_string(path.join("post-marker")).unwrap(),
        format!("{}:set", path.display())
    );
}

#[test]
fn a_failing_hook_reports_an_error() {
    let fx = Fixture::new();
    fx.write_config("[[hooks.post_create]]\ntype = \"command\"\ncommand = \"exit 7\"\n");

    let out = fx.gwx(["add", "broken"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("post_create hook failed"));
}

#[test]
fn a_copy_hook_keeps_symlinks_and_survives_broken_ones() {
    let fx = Fixture::new();
    let modules = fx.repo.join("node_modules");
    std::fs::create_dir_all(modules.join("pkg")).unwrap();
    std::fs::write(modules.join("pkg/index.js"), "1").unwrap();
    std::os::unix::fs::symlink("pkg/index.js", modules.join("bin")).unwrap();
    // What a removed package leaves behind. Following it used to fail the
    // hook and leave the copy half done.
    std::os::unix::fs::symlink("gone.js", modules.join("dangling")).unwrap();
    fx.write_config("[[hooks.post_create]]\ntype = \"copy\"\nfrom = \"node_modules\"\n");

    let path = PathBuf::from(fx.gwx_ok(["add", "linked"]));

    let copied = path.join("node_modules");
    assert!(copied.join("bin").symlink_metadata().unwrap().is_symlink());
    assert_eq!(
        std::fs::read_link(copied.join("bin")).unwrap(),
        Path::new("pkg/index.js")
    );
    assert!(copied
        .join("dangling")
        .symlink_metadata()
        .unwrap()
        .is_symlink());
    assert_eq!(
        std::fs::read_to_string(copied.join("pkg/index.js")).unwrap(),
        "1"
    );
}

#[test]
fn a_leftover_gwt_config_is_reported_rather_than_ignored() {
    let fx = Fixture::new();
    // What a repository configured for v1.4.1 and earlier looks like.
    std::fs::write(
        fx.repo.join(".gwt.toml"),
        "[[hooks.post_create]]\ntype = \"command\"\ncommand = \"true\"\n",
    )
    .unwrap();

    let out = fx.gwx(["add", "renamed"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(".gwt.toml"), "{stderr}");
    assert!(stderr.contains(".gwx.toml"), "{stderr}");
}

#[test]
fn no_hooks_skips_them() {
    let fx = Fixture::new();
    fx.write_config("[[hooks.post_create]]\ntype = \"command\"\ncommand = \"exit 7\"\n");
    fx.gwx_ok(["add", "fine", "--no-hooks"]);
}

#[test]
fn hooks_cannot_escape_the_worktree() {
    let fx = Fixture::new();
    fx.write_config("[[hooks.post_create]]\ntype = \"copy\"\nfrom = \"../outside\"\n");

    let out = fx.gwx(["add", "escape"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("escapes the worktree"));
}

#[test]
fn list_shows_every_worktree() {
    let fx = Fixture::new();
    fx.gwx_ok(["add", "feature/one"]);

    let listing = fx.gwx_ok(["list"]);
    assert!(listing.contains('@'), "{listing}");
    assert!(listing.contains("feature/one"), "{listing}");
    assert!(
        listing.lines().next().unwrap().starts_with('*'),
        "{listing}"
    );

    let paths = fx.gwx_ok(["list", "--paths"]);
    assert_eq!(paths.lines().count(), 2);
    assert!(paths.lines().any(|l| l == fx.repo.to_str().unwrap()));

    // `--plain` asks for the same table the picker would have replaced.
    assert_eq!(fx.gwx_ok(["list", "--plain"]), listing);
}

#[test]
fn a_chosen_directory_is_handed_over_through_a_file() {
    let fx = Fixture::new();
    let created = fx.gwx_ok(["add", "feature/handoff"]);
    let handoff = fx.root.join("cd-request");

    let out = command(BIN)
        .current_dir(&fx.repo)
        .env("GWX_CD_FILE", &handoff)
        .args(["cd", "feature/handoff"])
        .output()
        .expect("failed to run gwx");
    assert!(out.status.success());

    // The path goes to the file, and stdout stays empty so that a command
    // like `gwx list` can keep printing its own output.
    assert_eq!(
        std::fs::read_to_string(&handoff).unwrap().trim_end(),
        created
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
}

#[test]
fn list_stays_plain_without_a_terminal() {
    let fx = Fixture::new();
    fx.gwx_ok(["add", "feature/one"]);
    let handoff = fx.root.join("cd-request");

    // Even with the hand-off file offered, no terminal means no picker: the
    // table is printed and nothing asks the shell to move.
    let out = command(BIN)
        .current_dir(&fx.repo)
        .env("GWX_CD_FILE", &handoff)
        .arg("list")
        .output()
        .expect("failed to run gwx");

    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("feature/one"));
    assert!(!handoff.exists());
}

#[test]
fn an_ambient_git_dir_does_not_redirect_gwx() {
    let fx = Fixture::new();
    fx.gwx_ok(["add", "feature/here"]);

    // A second repository, standing in for the one a git hook would name.
    let elsewhere = fx.root.join("elsewhere");
    git(&fx.root, ["init", "--initial-branch=main", "elsewhere"]);

    // git reads GIT_DIR before it looks at the working directory, so without
    // care this asks about `elsewhere` while standing in `repo`.
    let out = command(BIN)
        .current_dir(&fx.repo)
        .env("GIT_DIR", elsewhere.join(".git"))
        .env("GIT_WORK_TREE", &elsewhere)
        .args(["list", "--paths"])
        .output()
        .expect("failed to run gwx");

    assert!(
        out.status.success(),
        "gwx failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let paths = String::from_utf8_lossy(&out.stdout);
    assert!(paths.contains("feature/here"), "{paths}");
    assert!(!paths.contains("elsewhere"), "{paths}");
}

#[test]
fn a_hook_does_not_inherit_a_pointer_to_another_repository() {
    let fx = Fixture::new();
    let elsewhere = fx.root.join("elsewhere");
    git(&fx.root, ["init", "--initial-branch=main", "elsewhere"]);
    fx.write_config(
        "[[hooks.post_create]]\ntype = \"command\"\ncommand = \"git rev-parse --show-toplevel > where\"\n",
    );

    let out = command(BIN)
        .current_dir(&fx.repo)
        .env("GIT_DIR", elsewhere.join(".git"))
        .env("GIT_WORK_TREE", &elsewhere)
        .args(["add", "feature/hooked"])
        .output()
        .expect("failed to run gwx");
    assert!(
        out.status.success(),
        "gwx failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The hook ran git inside the new worktree, not in `elsewhere`.
    let created = fx.worktrees_dir().join("feature/hooked");
    let where_it_ran = std::fs::read_to_string(created.join("where")).unwrap();
    assert_eq!(where_it_ran.trim(), created.to_str().unwrap());
}

#[test]
fn cd_resolves_names() {
    let fx = Fixture::new();
    let created = fx.gwx_ok(["add", "feature/two"]);

    assert_eq!(fx.gwx_ok(["cd", "feature/two"]), created);
    assert_eq!(fx.gwx_ok(["cd", "@"]), fx.repo.to_str().unwrap());
    // A bare `gwx cd` goes to the main worktree the way a bare `cd` goes home.
    // It never opens the picker — that is what `gwx list` is for.
    assert_eq!(fx.gwx_ok(["cd"]), fx.repo.to_str().unwrap());

    // Resolution also works from inside another worktree.
    let out = fx.gwx_in(Path::new(&created), ["cd", "@"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        fx.repo.to_str().unwrap()
    );
}

#[test]
fn cd_reports_unknown_names() {
    let fx = Fixture::new();
    let out = fx.gwx(["cd", "ghost"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no worktree named"));
}

#[test]
fn remove_deletes_the_worktree_and_optionally_the_branch() {
    let fx = Fixture::new();
    let path = PathBuf::from(fx.gwx_ok(["add", "feature/gone"]));

    let out = fx.gwx(["remove", "feature/gone", "--with-branch"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!path.exists());
    assert!(!git_out(&fx.repo, ["branch", "--list", "feature/gone"]).contains("feature/gone"));
}

#[test]
fn remove_protects_dirty_worktrees_and_the_main_one() {
    let fx = Fixture::new();
    let path = PathBuf::from(fx.gwx_ok(["add", "dirty"]));
    std::fs::write(path.join("README.md"), "changed\n").unwrap();

    let out = fx.gwx(["remove", "dirty"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("uncommitted changes"));
    assert!(path.exists());

    assert!(fx.gwx(["remove", "dirty", "--force"]).status.success());
    assert!(!path.exists());

    let out = fx.gwx(["remove", "@"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("main worktree"));
}

#[test]
fn remove_keeps_unmerged_branches() {
    let fx = Fixture::new();
    let path = PathBuf::from(fx.gwx_ok(["add", "unmerged"]));
    git(&path, ["config", "user.email", "test@example.com"]);
    git(&path, ["config", "user.name", "Test"]);
    std::fs::write(path.join("new.txt"), "x\n").unwrap();
    git(&path, ["add", "."]);
    git(&path, ["commit", "-m", "work"]);

    let out = fx.gwx(["remove", "unmerged", "--with-branch"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not merged"));
    // The worktree itself is gone; only the branch was kept.
    assert!(!path.exists());
    assert!(git_out(&fx.repo, ["branch", "--list", "unmerged"]).contains("unmerged"));
}

#[test]
fn hooks_run_before_and_after_removal() {
    let fx = Fixture::new();
    fx.write_config(
        r#"
version = "1"

[[hooks.pre_remove]]
type = "command"
command = "pwd -P > \"$GWX_MAIN_WORKTREE/pre-remove\""

[[hooks.post_remove]]
type = "command"
command = "printf %s \"$GWX_WORKTREE_NAME $GWX_BRANCH $GWX_WORKTREE_PATH\" > post-remove"
"#,
    );
    let path = PathBuf::from(fx.gwx_ok(["add", "feature/gone"]));

    let out = fx.gwx(["remove", "feature/gone", "--with-branch"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!path.exists());

    // pre_remove ran inside the worktree, while there still was one.
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("pre-remove")).unwrap(),
        format!("{}\n", path.display())
    );
    // post_remove ran in the main worktree, and still knows what went away.
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("post-remove")).unwrap(),
        format!("feature/gone feature/gone {}", path.display())
    );
}

#[test]
fn a_failing_pre_remove_hook_keeps_the_worktree() {
    let fx = Fixture::new();
    fx.write_config("[[hooks.pre_remove]]\ntype = \"command\"\ncommand = \"exit 7\"\n");
    let path = PathBuf::from(fx.gwx_ok(["add", "kept"]));

    let out = fx.gwx(["remove", "kept"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("pre_remove hook failed"));
    assert!(path.exists(), "the removal should have been called off");

    // The escape hatch still works when a hook is the thing that is broken.
    assert!(fx.gwx(["remove", "kept", "--no-hooks"]).status.success());
    assert!(!path.exists());
}

#[test]
fn a_failing_post_remove_hook_reports_the_removal_that_already_happened() {
    let fx = Fixture::new();
    fx.write_config("[[hooks.post_remove]]\ntype = \"command\"\ncommand = \"exit 7\"\n");
    let path = PathBuf::from(fx.gwx_ok(["add", "swept"]));

    let out = fx.gwx(["remove", "swept"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("post_remove hook failed"), "{stderr}");
    assert!(stderr.contains("the worktree was removed"), "{stderr}");
    assert!(!path.exists());
}

#[test]
fn copy_hooks_are_refused_around_removal() {
    let fx = Fixture::new();
    fx.write_config("[[hooks.pre_remove]]\ntype = \"copy\"\nfrom = \".env\"\n");
    let path = PathBuf::from(fx.gwx_ok(["add", "misconfigured"]));

    let out = fx.gwx(["remove", "misconfigured"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("only supported in post_create"));
    assert!(path.exists());
}

#[test]
fn init_writes_a_config_template() {
    let fx = Fixture::new();
    assert!(fx.gwx(["init"]).status.success());
    let contents = std::fs::read_to_string(fx.repo.join(".gwx.toml")).unwrap();
    assert!(contents.contains("base_dir"));

    let out = fx.gwx(["init"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("already exists"));
    assert!(fx.gwx(["init", "--force"]).status.success());
}

#[test]
fn shell_init_emits_a_wrapper_and_completions() {
    let fx = Fixture::new();
    for shell in ["bash", "zsh", "fish"] {
        let script = fx.gwx_ok(["shell-init", shell]);
        assert!(script.contains("gwx"), "{shell}: {script}");
        assert!(script.contains("cd"), "{shell}: {script}");
        // The completion stub calls back into the binary for candidates.
        assert!(script.contains("COMPLETE"), "{shell}: {script}");
    }
}

#[test]
fn cd_completes_worktree_names() {
    let fx = Fixture::new();
    fx.gwx_ok(["add", "feature/one"]);
    fx.gwx_ok(["add", "feature/two"]);

    let candidates = fx.complete(&["gwx", "cd", ""]);
    assert!(candidates.contains(&"@".to_string()), "{candidates:?}");
    assert!(
        candidates.contains(&"feature/one".to_string()),
        "{candidates:?}"
    );
    assert!(
        candidates.contains(&"feature/two".to_string()),
        "{candidates:?}"
    );

    // Typing a prefix narrows the list.
    let narrowed = fx.complete_in(&fx.repo, 2, &["gwx", "cd", "feature/o"]);
    assert!(
        narrowed.contains(&"feature/one".to_string()),
        "{narrowed:?}"
    );
    assert!(
        !narrowed.contains(&"feature/two".to_string()),
        "{narrowed:?}"
    );
}

#[test]
fn remove_completion_leaves_out_the_main_worktree() {
    let fx = Fixture::new();
    fx.gwx_ok(["add", "feature/one"]);

    let candidates = fx.complete(&["gwx", "remove", ""]);
    assert!(
        candidates.contains(&"feature/one".to_string()),
        "{candidates:?}"
    );
    assert!(!candidates.contains(&"@".to_string()), "{candidates:?}");
    assert!(!candidates.contains(&"main".to_string()), "{candidates:?}");
}

#[test]
fn add_completes_branches_without_a_worktree() {
    let fx = Fixture::new();
    git(&fx.repo, ["branch", "local-only"]);
    git(&fx.repo, ["branch", "remote-only"]);
    git(&fx.repo, ["push", "-q", "origin", "remote-only"]);
    git(&fx.repo, ["branch", "-D", "remote-only"]);
    fx.gwx_ok(["add", "taken"]);

    let candidates = fx.complete(&["gwx", "add", ""]);
    assert!(
        candidates.contains(&"local-only".to_string()),
        "{candidates:?}"
    );
    // Remote-only branches appear under the name `gwx add` expects.
    assert!(
        candidates.contains(&"remote-only".to_string()),
        "{candidates:?}"
    );
    assert!(
        !candidates.contains(&"origin/remote-only".to_string()),
        "{candidates:?}"
    );
    // Branches that already have a worktree would be rejected by `add`.
    assert!(!candidates.contains(&"taken".to_string()), "{candidates:?}");
    assert!(!candidates.contains(&"main".to_string()), "{candidates:?}");
}

#[test]
fn add_completion_lists_a_branch_once_per_name() {
    let fx = Fixture::new();
    fx.add_remote("upstream");

    // The same branch name on two remotes, and one that exists both locally
    // and on a remote.
    git(&fx.repo, ["branch", "dual"]);
    git(&fx.repo, ["push", "-q", "origin", "dual"]);
    git(&fx.repo, ["push", "-q", "upstream", "dual"]);
    git(&fx.repo, ["branch", "-D", "dual"]);
    git(&fx.repo, ["branch", "both"]);
    git(&fx.repo, ["push", "-q", "origin", "both"]);

    let candidates = fx.complete(&["gwx", "add", ""]);
    let count = |name: &str| candidates.iter().filter(|c| *c == name).count();

    assert_eq!(count("dual"), 1, "{candidates:?}");
    assert_eq!(count("both"), 1, "{candidates:?}");
}

#[test]
fn from_completes_branches_and_tags() {
    let fx = Fixture::new();
    git(&fx.repo, ["tag", "v1.0.0"]);

    let candidates = fx.complete(&["gwx", "add", "new-branch", "--from", ""]);
    assert!(candidates.contains(&"main".to_string()), "{candidates:?}");
    assert!(candidates.contains(&"v1.0.0".to_string()), "{candidates:?}");
    assert!(
        candidates.contains(&"origin/main".to_string()),
        "{candidates:?}"
    );
}

#[test]
fn completion_outside_a_repository_is_empty() {
    let fx = Fixture::new();
    let outside = fx.root.join("not-a-repo");
    std::fs::create_dir_all(&outside).unwrap();

    let candidates = fx.complete_in(&outside, 2, &["gwx", "cd", ""]);
    assert_eq!(candidates, vec!["--help".to_string()], "{candidates:?}");
}

#[test]
fn outside_a_repository_it_fails_cleanly() {
    let fx = Fixture::new();
    let outside = fx.root.join("not-a-repo");
    std::fs::create_dir_all(&outside).unwrap();

    let out = fx.gwx_in(&outside, ["list"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not inside a git repository"));
}

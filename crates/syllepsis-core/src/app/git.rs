//! Git convenience commands for deliberate book snapshots.
//!
//! Git is intentionally treated as an installed external tool, not a library dependency or a
//! bundled runtime. Complex failures are surfaced so users can switch to a terminal.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::app::publish;
use crate::error::{CoreError, CoreResult};
use crate::storage::{layout, Book, NoteStore};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStatusDto {
    pub available: bool,
    pub version: Option<String>,
    pub is_repository: bool,
    pub branch: Option<String>,
    pub changed_files: Vec<GitChangedFile>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitChangedFile {
    pub path: String,
    pub status: String,
    pub stage_by_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommandReport {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub hint: Option<String>,
}

pub fn git_status(book: &Book) -> CoreResult<GitStatusDto> {
    git_status_with_bin(book, "git")
}

pub fn git_stage_commit(
    book: &Book,
    selected_paths: &[String],
    message: &str,
) -> CoreResult<GitCommandReport> {
    git_stage_commit_with_bin(book, "git", selected_paths, message)
}

pub fn git_push(book: &Book) -> CoreResult<GitCommandReport> {
    run_git_report(&book.root, "git", &["push"])
}

pub fn git_pull(book: &Book) -> CoreResult<GitCommandReport> {
    run_git_report(&book.root, "git", &["pull"])
}

fn git_status_with_bin(book: &Book, git_bin: &str) -> CoreResult<GitStatusDto> {
    let version = match run_git(&book.root, git_bin, &["--version"]) {
        Ok(output) => output.stdout.trim().to_string(),
        Err(error) => {
            return Ok(GitStatusDto {
                available: false,
                version: None,
                is_repository: false,
                branch: None,
                changed_files: Vec::new(),
                error: Some(error.to_string()),
            });
        }
    };
    let is_repository = run_git(&book.root, git_bin, &["rev-parse", "--is-inside-work-tree"])
        .map(|output| output.stdout.trim() == "true")
        .unwrap_or(false);
    if !is_repository {
        return Ok(GitStatusDto {
            available: true,
            version: Some(version),
            is_repository: false,
            branch: None,
            changed_files: Vec::new(),
            error: Some("book folder is not a git repository".to_string()),
        });
    }

    let branch = run_git(&book.root, git_bin, &["branch", "--show-current"])
        .ok()
        .map(|output| output.stdout.trim().to_string())
        .filter(|branch| !branch.is_empty());
    let ignored_private_paths = publish::refresh_private_gitignore(book)?.excluded_paths;
    let changed_files = parse_status(
        &run_git(&book.root, git_bin, &["status", "--porcelain=v1"])?.stdout,
        &ignored_private_paths,
    );
    Ok(GitStatusDto {
        available: true,
        version: Some(version),
        is_repository: true,
        branch,
        changed_files,
        error: None,
    })
}

fn git_stage_commit_with_bin(
    book: &Book,
    git_bin: &str,
    selected_paths: &[String],
    message: &str,
) -> CoreResult<GitCommandReport> {
    if message.trim().is_empty() {
        return Err(CoreError::Sync("git commit message is required".into()));
    }
    let ignored_private_paths = publish::refresh_private_gitignore(book)?.excluded_paths;
    let ignored_private_paths = ignored_private_paths
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if selected_paths.is_empty() {
        return Err(CoreError::Sync("select at least one path to commit".into()));
    }

    let allowed = book
        .store
        .read_all_notes()?
        .into_iter()
        .map(|note| layout::note_filename(&note.id))
        .chain([
            layout::BOOK_META_FILE.to_string(),
            layout::CONFIG_FILE.to_string(),
            ".gitignore".to_string(),
        ])
        .collect::<std::collections::BTreeSet<_>>();

    let paths = selected_paths
        .iter()
        .filter(|path| {
            allowed.contains(path.as_str()) && !ignored_private_paths.contains(path.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Err(CoreError::Sync(
            "selected paths did not include any commit-safe note files".into(),
        ));
    }

    let mut add_args = vec!["add", "--"];
    let path_refs = paths.iter().map(String::as_str).collect::<Vec<_>>();
    add_args.extend(path_refs.iter().copied());
    run_git_report(&book.root, git_bin, &add_args)?;
    run_git_report(&book.root, git_bin, &["commit", "-m", message.trim()])
}

fn parse_status(status: &str, ignored_private_paths: &[String]) -> Vec<GitChangedFile> {
    let ignored = ignored_private_paths
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    status
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            let status = line[..2].trim().to_string();
            let path = line[3..].trim().to_string();
            let stage_by_default = path.ends_with(".md")
                && !path.starts_with("_")
                && !path.contains(".conflict-")
                && !ignored.contains(path.as_str());
            Some(GitChangedFile {
                path,
                status,
                stage_by_default,
            })
        })
        .collect()
}

fn run_git_report(cwd: &Path, git_bin: &str, args: &[&str]) -> CoreResult<GitCommandReport> {
    let output = run_git(cwd, git_bin, args)?;
    if !output.status.success() {
        return Err(CoreError::Sync(format!(
            "git {} failed with status {}\nstdout:\n{}\nstderr:\n{}\n{}",
            args.join(" "),
            output.status,
            output.stdout,
            output.stderr,
            git_hint(&output.stderr).unwrap_or_default()
        )));
    }
    Ok(GitCommandReport {
        command: format!("git {}", args.join(" ")),
        stdout: output.stdout,
        stderr: output.stderr,
        hint: None,
    })
}

struct GitOutput {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn run_git(cwd: &Path, git_bin: &str, args: &[&str]) -> CoreResult<GitOutput> {
    let output = Command::new(git_bin)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| CoreError::Sync(format!("run git {}: {e}", args.join(" "))))?;
    Ok(GitOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn git_hint(stderr: &str) -> Option<String> {
    let lower = stderr.to_lowercase();
    if lower.contains("merge conflict") || lower.contains("conflict") {
        Some("Resolve the conflict in a terminal or dedicated Git client, then retry.".to_string())
    } else if lower.contains("no upstream") {
        Some("Set an upstream branch from the command line, then retry push.".to_string())
    } else if lower.contains("not possible to fast-forward") {
        Some("Pull/rebase from the command line, then retry.".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ObjectType;

    #[test]
    fn unavailable_git_reports_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path().join("b"), "B").unwrap();
        let status = git_status_with_bin(&book, "definitely-not-git").unwrap();
        assert!(!status.available);
        assert!(status.error.unwrap().contains("run git"));
    }

    #[test]
    fn selected_note_files_are_committed() {
        let dir = tempfile::tempdir().unwrap();
        let book = Book::create(dir.path().join("b"), "B").unwrap();
        if run_git(&book.root, "git", &["--version"]).is_err() {
            return;
        }
        run_git_report(&book.root, "git", &["init"]).unwrap();
        run_git_report(
            &book.root,
            "git",
            &["config", "user.email", "test@example.com"],
        )
        .unwrap();
        run_git_report(&book.root, "git", &["config", "user.name", "Test"]).unwrap();
        let mut note = book.new_note(ObjectType::Note, "public").unwrap();
        note.body = "body".into();
        book.save_note(&note).unwrap();
        let private = book.new_note(ObjectType::Note, "private").unwrap();
        crate::app::lifecycle::set_note_private(&book, private.id.as_str(), true).unwrap();

        let report = git_stage_commit_with_bin(
            &book,
            "git",
            &[
                layout::note_filename(&note.id),
                layout::note_filename(&private.id),
            ],
            "snapshot",
        )
        .unwrap();

        assert!(report.stdout.contains("snapshot") || report.stderr.is_empty());
        let tracked = run_git(&book.root, "git", &["ls-files"]).unwrap().stdout;
        assert!(tracked.contains(&layout::note_filename(&note.id)));
        assert!(!tracked.contains(&layout::note_filename(&private.id)));
    }
}

//! Central git-repo sync for hoppr's config file.
//!
//! Workflow:
//!   * On startup, if `sync.repo` is configured, hoppr ensures a local clone of
//!     that repo exists at `sync.local`. With `auto_pull = true` (the default
//!     when sync is enabled) it fast-forward-pulls the configured branch.
//!   * The active config file is symlinked from the clone if it isn't already
//!     present locally; otherwise it is copied. Hand-edits to the local file
//!     are pushed via `hoppr sync push` (or `auto_push: true`).
//!
//! libgit2 is used (via the `git2` crate, vendored libgit2 + openssl) so we
//! don't depend on the system `git` binary at runtime.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use git2::{
    AnnotatedCommit, BranchType, Cred, FetchOptions, ObjectType, PushOptions, RemoteCallbacks,
    Repository, Signature,
};

use crate::config::{default_sync_local, SyncConfig};

#[derive(Debug, Clone)]
pub struct SyncContext {
    pub repo_url: String,
    pub branch: String,
    pub file_in_repo: String,
    pub local_clone: PathBuf,
    pub auto_pull: bool,
    #[allow(dead_code)]
    pub auto_push: bool,
}

impl SyncContext {
    pub fn from(sync: &SyncConfig) -> Option<Self> {
        let repo_url = sync.repo.clone()?;
        let file_in_repo = sync
            .path
            .clone()
            .filter(|p| !contains_parent_traversal(p))
            .unwrap_or_else(|| "config.yaml".into());
        Some(Self {
            repo_url,
            branch: sync.branch.clone().unwrap_or_else(|| "main".into()),
            file_in_repo,
            local_clone: sync
                .local
                .as_deref()
                .map(expand_path)
                .unwrap_or_else(default_sync_local),
            auto_pull: sync.auto_pull.unwrap_or(true),
            auto_push: sync.auto_push.unwrap_or(false),
        })
    }

    pub fn tracked_path(&self) -> PathBuf {
        self.local_clone.join(&self.file_in_repo)
    }

    /// Repo URL with any userinfo (`user:token@`) stripped — safe to use in
    /// error messages and logs.
    pub fn safe_url(&self) -> String {
        redact_url(&self.repo_url)
    }
}

fn contains_parent_traversal(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Strip `user:password@` (and `user@`) prefixes from an http(s) URL so secrets
/// aren't echoed in error messages. SSH URLs (`git@host:...`) are left intact
/// — the `git` username is conventional and there's no embedded secret.
fn redact_url(url: &str) -> String {
    if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        let scheme = if url.starts_with("https://") {
            "https://"
        } else {
            "http://"
        };
        if let Some(idx) = rest.find('@') {
            return format!("{scheme}{}", &rest[idx + 1..]);
        }
    }
    url.to_string()
}

fn expand_path(input: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(input).into_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Disabled,
    UpToDate,
    Pulled,
    PulledWithChanges,
    Skipped,
    Failed,
}

/// Build a fresh set of `RemoteCallbacks` wired to our credentials
/// helper.
///
/// SSH is a two-round flow: libgit2 first asks for the *username*
/// (separate from the credential), then for the actual key. We answer
/// USERNAME requests once each operation, then defer the real auth
/// step to `credentials_cb`. A second real-auth request means our
/// credentials were rejected — we bail out instead of looping forever.
fn make_auth_callbacks() -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();
    let mut answered_username = false;
    let mut answered_credential = false;
    callbacks.credentials(move |url, user, allowed| {
        if allowed.contains(git2::CredentialType::USERNAME) {
            if answered_username {
                return Err(git2::Error::from_str("username re-requested"));
            }
            answered_username = true;
            return Cred::username(user.unwrap_or("git"));
        }
        if answered_credential {
            return Err(git2::Error::from_str("credentials already attempted"));
        }
        answered_credential = true;
        credentials_cb(url, user, allowed)
    });
    callbacks
}

impl SyncStatus {
    pub fn label(self) -> &'static str {
        match self {
            SyncStatus::Disabled => "sync off",
            SyncStatus::UpToDate => "sync ok",
            SyncStatus::Pulled => "pulled",
            SyncStatus::PulledWithChanges => "pulled +",
            SyncStatus::Skipped => "sync skipped",
            SyncStatus::Failed => "sync error",
        }
    }
}

/// Probe the configured remote without writing anything to disk.
///
/// Uses libgit2's `ls_remote` against an in-memory remote so the same
/// credential callbacks are exercised that a real clone or pull would
/// hit. Useful from the settings editor as a "test connection" button.
pub fn test_connection(repo_url: &str) -> Result<()> {
    let mut remote = git2::Remote::create_detached(repo_url)
        .with_context(|| format!("invalid repo URL: {}", redact_url(repo_url)))?;
    remote
        .connect_auth(git2::Direction::Fetch, Some(make_auth_callbacks()), None)
        .with_context(|| format!("failed to reach {}", redact_url(repo_url)))?;
    remote.disconnect().ok();
    Ok(())
}

/// Clone the configured repo if missing, otherwise fast-forward pull.
pub fn ensure_repo(ctx: &SyncContext) -> Result<SyncStatus> {
    let needs_clone = match local_clone_state(&ctx.local_clone) {
        LocalCloneState::Absent => true,
        LocalCloneState::ValidRepo => false,
        LocalCloneState::SafelyReplaceable => {
            // Most often the residue of a failed first-time clone — wipe
            // and try again so the user doesn't have to drop to a shell.
            fs::remove_dir_all(&ctx.local_clone).with_context(|| {
                format!(
                    "failed to clean up partial clone at {}",
                    ctx.local_clone.display()
                )
            })?;
            true
        }
        LocalCloneState::OccupiedNonRepo => {
            return Err(anyhow!(
                "{} exists but is not a git repository, and is not empty — refusing to clobber. \
                 Remove the directory manually or point sync.local at a different path.",
                ctx.local_clone.display()
            ));
        }
    };

    if needs_clone {
        if let Some(parent) = ctx.local_clone.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create sync parent dir: {}", parent.display())
            })?;
        }
        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(make_auth_callbacks());

        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch_opts);
        builder.branch(&ctx.branch);
        builder
            .clone(&ctx.repo_url, &ctx.local_clone)
            .with_context(|| format!("failed to clone {}", ctx.safe_url()))?;
        return Ok(SyncStatus::Pulled);
    }

    if !ctx.auto_pull {
        return Ok(SyncStatus::Skipped);
    }

    pull(ctx)
}

/// What's currently sitting at the local-clone path. Used by
/// [`ensure_repo`] to decide between "clone fresh", "pull", "wipe + re-clone"
/// and "refuse to touch".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalCloneState {
    /// Path doesn't exist yet.
    Absent,
    /// Path is a directory that libgit2 can open as a repository.
    ValidRepo,
    /// Path is a directory but not a usable repo, and only contains
    /// `.git/` entries (or is empty) — looks like a half-clone we can
    /// safely wipe and retry.
    SafelyReplaceable,
    /// Path exists with unknown contents — needs manual intervention so
    /// hoppr doesn't trash someone's files.
    OccupiedNonRepo,
}

fn local_clone_state(path: &Path) -> LocalCloneState {
    if !path.exists() {
        return LocalCloneState::Absent;
    }
    if Repository::open(path).is_ok() {
        return LocalCloneState::ValidRepo;
    }
    if dir_holds_only_git_traces(path) {
        LocalCloneState::SafelyReplaceable
    } else {
        LocalCloneState::OccupiedNonRepo
    }
}

/// True when the directory is empty or only contains a (potentially
/// broken) `.git` entry — the recoverable residue of a failed clone.
fn dir_holds_only_git_traces(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        if entry.file_name() != ".git" {
            return false;
        }
    }
    true
}

/// True when libgit2 can open the configured clone as a repository.
/// Surfaced so callers don't have to import `git2` just to gate on
/// "is the local clone usable".
pub fn local_repo_ready(ctx: &SyncContext) -> bool {
    ctx.local_clone.exists() && Repository::open(&ctx.local_clone).is_ok()
}

/// Fast-forward pull. Returns whether the working copy moved.
pub fn pull(ctx: &SyncContext) -> Result<SyncStatus> {
    let repo = Repository::open(&ctx.local_clone)
        .with_context(|| format!("failed to open repo: {}", ctx.local_clone.display()))?;

    {
        let mut remote = repo
            .find_remote("origin")
            .context("missing origin remote")?;

        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(make_auth_callbacks());

        remote
            .fetch(&[ctx.branch.as_str()], Some(&mut fetch_opts), None)
            .with_context(|| format!("failed to fetch {} {}", ctx.safe_url(), ctx.branch))?;
        remote.disconnect().ok();
    }

    let fetch_head = repo
        .find_reference("FETCH_HEAD")
        .context("FETCH_HEAD missing")?;
    let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;
    let analysis = repo.merge_analysis(&[&fetch_commit])?;

    if analysis.0.is_up_to_date() {
        return Ok(SyncStatus::UpToDate);
    }

    if analysis.0.is_fast_forward() {
        fast_forward(&repo, &ctx.branch, &fetch_commit)?;
        return Ok(SyncStatus::PulledWithChanges);
    }

    Err(anyhow!(
        "non fast-forward pull required for branch {} — resolve manually",
        ctx.branch
    ))
}

fn fast_forward(repo: &Repository, branch: &str, fetch_commit: &AnnotatedCommit<'_>) -> Result<()> {
    let refname = format!("refs/heads/{branch}");
    let mut reference = repo
        .find_reference(&refname)
        .with_context(|| format!("local branch missing: {branch}"))?;

    reference.set_target(fetch_commit.id(), "hoppr fast-forward")?;
    repo.set_head(&refname)?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
    Ok(())
}

/// Stage, commit and push the tracked config file.
pub fn commit_and_push(ctx: &SyncContext, message: &str) -> Result<()> {
    let repo = Repository::open(&ctx.local_clone)
        .with_context(|| format!("failed to open repo: {}", ctx.local_clone.display()))?;

    let tracked = Path::new(&ctx.file_in_repo);
    let mut index = repo.index()?;
    index.add_path(tracked)?;
    index.write()?;

    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    let signature = make_signature(&repo)?;

    let parent_commit = match repo.head() {
        Ok(head) => Some(head.peel_to_commit()?),
        Err(_) => None,
    };

    if let Some(parent) = &parent_commit {
        if parent.tree()?.id() == tree.id() {
            return Ok(());
        }
    }

    let parents: Vec<&git2::Commit> = parent_commit.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )?;

    let mut remote = repo
        .find_remote("origin")
        .context("missing origin remote")?;
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(make_auth_callbacks());

    let refspec = format!("refs/heads/{0}:refs/heads/{0}", ctx.branch);
    remote
        .push(&[refspec.as_str()], Some(&mut push_opts))
        .context("git push failed")?;
    Ok(())
}

fn make_signature(repo: &Repository) -> Result<Signature<'static>> {
    if let Ok(sig) = repo.signature() {
        let name = sig.name().unwrap_or("hoppr").to_string();
        let email = sig.email().unwrap_or("hoppr@localhost").to_string();
        return Ok(Signature::now(&name, &email)?);
    }
    Signature::now("hoppr", "hoppr@localhost").map_err(Into::into)
}

/// Try a sequence of credential helpers — SSH agent, default key files,
/// the OS git config, finally any USERNAME/PASSWORD env vars.
fn credentials_cb(
    url: &str,
    username: Option<&str>,
    allowed: git2::CredentialType,
) -> Result<Cred, git2::Error> {
    if allowed.contains(git2::CredentialType::SSH_KEY) {
        let user = username.unwrap_or("git");
        if let Ok(cred) = Cred::ssh_key_from_agent(user) {
            return Ok(cred);
        }
        if let Some(home) = directories::UserDirs::new() {
            let ssh_dir = home.home_dir().join(".ssh");
            for name in ["id_ed25519", "id_rsa", "id_ecdsa"] {
                let key = ssh_dir.join(name);
                if key.exists() {
                    return Cred::ssh_key(user, None, &key, None);
                }
            }
        }
    }

    if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
        if let (Ok(user), Ok(pass)) = (
            std::env::var("HOPPR_GIT_USER"),
            std::env::var("HOPPR_GIT_TOKEN"),
        ) {
            return Cred::userpass_plaintext(&user, &pass);
        }
        if let Ok(config) = git2::Config::open_default() {
            if let Ok(helper) = Cred::credential_helper(&config, url, username) {
                return Ok(helper);
            }
        }
    }

    if allowed.contains(git2::CredentialType::DEFAULT) {
        return Cred::default();
    }

    Err(git2::Error::from_str("no suitable credentials available"))
}

/// Branch convenience helper used by `hoppr sync status`.
pub fn current_branch(ctx: &SyncContext) -> Result<String> {
    let repo = Repository::open(&ctx.local_clone)?;
    let head = repo.head()?;
    if head.is_branch() {
        let branch = git2::Branch::wrap(head);
        Ok(branch.name()?.unwrap_or("HEAD").to_string())
    } else {
        Ok("DETACHED".into())
    }
}

/// Returns true when the local clone has uncommitted changes.
pub fn has_uncommitted_changes(ctx: &SyncContext) -> Result<bool> {
    let repo = Repository::open(&ctx.local_clone)?;
    let statuses = repo.statuses(None)?;
    Ok(statuses
        .iter()
        .any(|s| !s.status().is_ignored() && s.status() != git2::Status::CURRENT))
}

#[allow(dead_code)]
pub fn checkout_branch(ctx: &SyncContext) -> Result<()> {
    let repo = Repository::open(&ctx.local_clone)?;
    let _ = repo.find_branch(&ctx.branch, BranchType::Local)?;
    let obj = repo.revparse_single(&format!("refs/heads/{}", ctx.branch))?;
    repo.checkout_tree(&obj, None)?;
    repo.set_head(&format!("refs/heads/{}", ctx.branch))?;
    Ok(())
}

#[allow(dead_code)]
fn object_kind_label(kind: Option<ObjectType>) -> &'static str {
    match kind {
        Some(ObjectType::Commit) => "commit",
        Some(ObjectType::Tree) => "tree",
        Some(ObjectType::Blob) => "blob",
        Some(ObjectType::Tag) => "tag",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_https_credentials() {
        assert_eq!(
            redact_url("https://alice:secret@github.com/x/y.git"),
            "https://github.com/x/y.git"
        );
        assert_eq!(
            redact_url("https://token@github.com/x/y.git"),
            "https://github.com/x/y.git"
        );
    }

    #[test]
    fn leaves_plain_urls_alone() {
        assert_eq!(
            redact_url("https://github.com/x/y.git"),
            "https://github.com/x/y.git"
        );
        assert_eq!(
            redact_url("git@github.com:x/y.git"),
            "git@github.com:x/y.git"
        );
        assert_eq!(
            redact_url("ssh://git@github.com/x"),
            "ssh://git@github.com/x"
        );
    }

    #[test]
    fn rejects_parent_traversal_in_repo_path() {
        let sync = SyncConfig {
            repo: Some("git@github.com:x/y.git".into()),
            path: Some("../../etc/passwd".into()),
            ..Default::default()
        };
        let ctx = SyncContext::from(&sync).unwrap();
        // Falls back to the safe default instead of honoring `..`.
        assert_eq!(ctx.file_in_repo, "config.yaml");
    }

    #[test]
    fn local_clone_state_classifies_absent_dir() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let missing = tmp.path().join("does-not-exist");
        assert_eq!(local_clone_state(&missing), LocalCloneState::Absent);
    }

    #[test]
    fn local_clone_state_classifies_empty_dir_as_replaceable() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        assert_eq!(
            local_clone_state(tmp.path()),
            LocalCloneState::SafelyReplaceable
        );
    }

    #[test]
    fn local_clone_state_classifies_dir_with_only_broken_git_as_replaceable() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        // A failed clone often leaves an empty .git/ behind that
        // Repository::open can't load.
        fs::create_dir(tmp.path().join(".git")).expect("create .git");
        assert_eq!(
            local_clone_state(tmp.path()),
            LocalCloneState::SafelyReplaceable
        );
    }

    #[test]
    fn local_clone_state_refuses_to_clobber_unknown_contents() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        fs::write(tmp.path().join("notes.txt"), "important user data").expect("write");
        assert_eq!(
            local_clone_state(tmp.path()),
            LocalCloneState::OccupiedNonRepo
        );
    }

    #[test]
    fn local_clone_state_recognises_real_git_repo() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        Repository::init(tmp.path()).expect("init repo");
        assert_eq!(local_clone_state(tmp.path()), LocalCloneState::ValidRepo);
    }
}

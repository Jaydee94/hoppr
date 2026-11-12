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
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::{Context, Result, anyhow};
use git2::{
    AnnotatedCommit, Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository, Signature,
};

use crate::config::{SyncConfig, default_sync_local};

#[derive(Debug, Clone)]
pub struct SyncContext {
    pub repo_url: String,
    pub branch: String,
    pub file_in_repo: String,
    pub local_clone: PathBuf,
    pub auto_pull: bool,
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
                .map_or_else(default_sync_local, expand_path),
            auto_pull: sync.auto_pull.unwrap_or(true),
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

/// Strip embedded credentials from a `scheme://` URL so secrets aren't
/// echoed in error messages. For http(s) the whole userinfo goes, because a
/// lone `token@` is itself the secret. For every other scheme (`ssh://`,
/// `git://`, …) only `:password` is dropped: the user name selects the
/// account and is not a secret. scp-style `git@host:path` carries no
/// password and is left intact.
///
/// Userinfo only exists inside the authority, which ends at the first `/`,
/// `?` or `#`; an `@` further along belongs to the path or query.
pub fn redact_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let is_scheme = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !is_scheme {
        return url.to_string();
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let Some(at) = rest[..authority_end].rfind('@') else {
        return url.to_string();
    };
    let after_userinfo = &rest[at + 1..];
    if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        return format!("{scheme}://{after_userinfo}");
    }
    match rest[..at].split_once(':') {
        Some((user, _)) if !user.is_empty() => format!("{scheme}://{user}@{after_userinfo}"),
        Some(_) => format!("{scheme}://{after_userinfo}"),
        None => url.to_string(),
    }
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

/// One way of answering libgit2's credential request, in the order they are
/// tried.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AuthAttempt {
    Agent,
    KeyFile(PathBuf),
    EnvToken,
    CredentialHelper,
    Default,
}

/// Everything worth offering for `allowed`, most preferred first: SSH agent,
/// each private key file, the `HOPPR_GIT_*` token, the OS credential helper.
fn auth_plan(
    allowed: git2::CredentialType,
    key_files: &[PathBuf],
    env_token_set: bool,
) -> Vec<AuthAttempt> {
    let mut plan = Vec::new();
    if allowed.contains(git2::CredentialType::SSH_KEY) {
        plan.push(AuthAttempt::Agent);
        plan.extend(key_files.iter().cloned().map(AuthAttempt::KeyFile));
    }
    if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
        if env_token_set {
            plan.push(AuthAttempt::EnvToken);
        }
        plan.push(AuthAttempt::CredentialHelper);
    }
    if allowed.contains(git2::CredentialType::DEFAULT) {
        plan.push(AuthAttempt::Default);
    }
    plan
}

/// Walks the [`auth_plan`] across repeated credential requests.
///
/// libgit2 calls the credentials callback again each time the server rejects
/// what it was given, and keeps doing so for as long as it receives something
/// new. Handing out each attempt at most once is what lets a later source get
/// its turn after an earlier one fails (no agent running, key not authorised)
/// and what ends the exchange once the plan is used up.
#[derive(Debug, Default)]
struct AuthSequence {
    offered: Vec<AuthAttempt>,
}

impl AuthSequence {
    /// The allowed types can change between requests, so the plan is rebuilt
    /// each time and matched against what was already offered rather than
    /// indexed by a counter.
    fn next(
        &mut self,
        allowed: git2::CredentialType,
        key_files: &[PathBuf],
        env_token_set: bool,
    ) -> Option<AuthAttempt> {
        let attempt = auth_plan(allowed, key_files, env_token_set)
            .into_iter()
            .find(|attempt| !self.offered.contains(attempt))?;
        self.offered.push(attempt.clone());
        Some(attempt)
    }
}

/// Build a fresh set of `RemoteCallbacks` wired to our credentials
/// helper.
///
/// SSH is a two-round flow: libgit2 first asks for the *username*
/// (separate from the credential), then for the actual key. USERNAME
/// requests are idempotent — libgit2 may legitimately ask for it more
/// than once across a single operation (e.g. `test_connection` on some
/// libssh2 builds), so we just keep answering. Every further request
/// means the previous answer was rejected and moves one step along the
/// [`AuthSequence`]; when it runs dry we bail out instead of spinning
/// forever.
fn make_auth_callbacks() -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();
    let mut sequence = AuthSequence::default();
    let mut key_files: Option<Vec<PathBuf>> = None;
    callbacks.credentials(move |url, user, allowed| {
        if allowed.contains(git2::CredentialType::USERNAME) {
            return Cred::username(user.unwrap_or("git"));
        }
        let key_files = key_files.get_or_insert_with(default_key_files);
        while let Some(attempt) = sequence.next(allowed, key_files, env_token().is_some()) {
            if let Ok(cred) = build_credential(&attempt, url, user) {
                return Ok(cred);
            }
        }
        Err(git2::Error::from_str(if sequence.offered.is_empty() {
            "no suitable credentials available"
        } else {
            "every available credential was rejected"
        }))
    });
    // Explicit passthrough: defer to libgit2's defaults (known_hosts for
    // SSH, system CA roots for HTTPS). Without this, some libgit2 builds
    // skip the host check entirely for SSH detached remotes, which would
    // make the test button report success against unreachable hosts.
    callbacks
        .certificate_check(|_cert, _host| Ok(git2::CertificateCheckStatus::CertificatePassthrough));
    callbacks
}

impl SyncStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "sync off",
            Self::UpToDate => "sync ok",
            Self::Pulled => "pulled",
            Self::PulledWithChanges => "pulled +",
            Self::Skipped => "sync skipped",
            Self::Failed => "sync error",
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
    // Force a ref listing — for SSH transports this is what actually
    // exercises authentication end-to-end; without it some libgit2 builds
    // would report the connect as successful even when auth would later
    // fail on a real fetch/push.
    let _ = remote.list().with_context(|| {
        format!(
            "connected but could not list refs on {}",
            redact_url(repo_url)
        )
    })?;
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

/// Cheap (local-only) probe of the synced clone. Returns `None` when sync
/// isn't configured or the clone isn't a usable git repo; otherwise the
/// bool reflects `git status` — true means there are uncommitted changes.
pub fn probe_dirty(sync: Option<&SyncConfig>) -> Option<bool> {
    let ctx = SyncContext::from(sync?)?;
    if !local_repo_ready(&ctx) {
        return None;
    }
    has_uncommitted_changes(&ctx).ok()
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
///
/// Two libgit2 quirks worth knowing about:
///
/// * If the working-tree file already matches `HEAD`, we skip the commit
///   but **still attempt the push** — there may be unpushed local
///   commits left over from a previous round that silently failed.
/// * `remote.push` returns `Ok` even when the server rejects the ref
///   update. We install a `push_update_reference` callback to capture
///   per-ref status and turn rejections into hard errors so callers
///   (and the user's status bar) see the real result.
pub fn commit_and_push(ctx: &SyncContext, message: &str) -> Result<()> {
    let repo = Repository::open(&ctx.local_clone)
        .with_context(|| format!("failed to open repo: {}", ctx.local_clone.display()))?;

    let tracked = Path::new(&ctx.file_in_repo);
    let mut index = repo.index()?;
    index.add_path(tracked)?;
    index.write()?;

    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    let parent_commit = match repo.head() {
        Ok(head) => Some(head.peel_to_commit()?),
        Err(_) => None,
    };

    let needs_commit = match &parent_commit {
        Some(parent) => parent.tree()?.id() != tree.id(),
        None => true,
    };

    if needs_commit {
        let signature = make_signature(&repo)?;
        let parents: Vec<&git2::Commit> = parent_commit.iter().collect();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )?;
    }

    let mut remote = repo
        .find_remote("origin")
        .context("missing origin remote")?;

    let rejections: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let mut callbacks = make_auth_callbacks();
    {
        let rejections = rejections.clone();
        callbacks.push_update_reference(move |refname, status| {
            if let Some(msg) = status {
                rejections.borrow_mut().push(format!("{refname}: {msg}"));
            }
            Ok(())
        });
    }
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);

    let refspec = format!("refs/heads/{0}:refs/heads/{0}", ctx.branch);
    remote
        .push(&[refspec.as_str()], Some(&mut push_opts))
        .with_context(|| format!("git push to {} failed", ctx.safe_url()))?;

    let rejections = rejections.borrow();
    if !rejections.is_empty() {
        return Err(anyhow!(
            "remote rejected the push: {}",
            rejections.join("; ")
        ));
    }
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

fn env_token() -> Option<(String, String)> {
    let user = std::env::var("HOPPR_GIT_USER").ok()?;
    let token = std::env::var("HOPPR_GIT_TOKEN").ok()?;
    Some((user, token))
}

/// Constructing an agent or key-file credential never fails for a missing
/// agent or an unusable key — libgit2 only finds out when it authenticates
/// with it, and reports that by asking for credentials again.
fn build_credential(
    attempt: &AuthAttempt,
    url: &str,
    username: Option<&str>,
) -> Result<Cred, git2::Error> {
    let ssh_user = username.unwrap_or("git");
    match attempt {
        AuthAttempt::Agent => Cred::ssh_key_from_agent(ssh_user),
        AuthAttempt::KeyFile(path) => Cred::ssh_key(ssh_user, None, path, None),
        AuthAttempt::EnvToken => {
            let (user, token) = env_token()
                .ok_or_else(|| git2::Error::from_str("HOPPR_GIT_USER/HOPPR_GIT_TOKEN unset"))?;
            Cred::userpass_plaintext(&user, &token)
        }
        AuthAttempt::CredentialHelper => {
            let config = git2::Config::open_default()?;
            Cred::credential_helper(&config, url, username)
        }
        AuthAttempt::Default => Cred::default(),
    }
}

fn default_key_files() -> Vec<PathBuf> {
    directories::UserDirs::new()
        .map(|dirs| private_key_files(&dirs.home_dir().join(".ssh")))
        .unwrap_or_default()
}

/// Private keys in `ssh_dir`: the standard filenames first (so a
/// less-likely candidate is not picked ahead of them), then any other
/// `id_*` file — covers custom-named keys like `id_work` — in name order.
fn private_key_files(ssh_dir: &Path) -> Vec<PathBuf> {
    const STANDARD: [&str; 4] = ["id_ed25519", "id_rsa", "id_ecdsa", "id_dsa"];
    let mut keys: Vec<PathBuf> = STANDARD
        .iter()
        .map(|name| ssh_dir.join(name))
        .filter(|path| path.is_file())
        .collect();
    let mut custom: Vec<PathBuf> = fs::read_dir(ssh_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_private_key_candidate(path) && !keys.contains(path))
        .collect();
    custom.sort();
    keys.extend(custom);
    keys
}

/// True for `~/.ssh` entries named `id_*` that are not the public half of
/// a key pair (`.pub`, in any letter case).
fn is_private_key_candidate(path: &Path) -> bool {
    let is_id_file = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("id_"));
    let is_public_key = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pub"));
    is_id_file && !is_public_key
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

pub fn has_uncommitted_changes(ctx: &SyncContext) -> Result<bool> {
    let repo = Repository::open(&ctx.local_clone)?;
    let statuses = repo.statuses(None)?;
    Ok(statuses
        .iter()
        .any(|s| !s.status().is_ignored() && s.status() != git2::Status::CURRENT))
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
    fn redacts_passwords_but_keeps_the_user_for_other_schemes() {
        assert_eq!(
            redact_url("ssh://git:secret@host.example/team/repo.git"),
            "ssh://git@host.example/team/repo.git"
        );
        assert_eq!(
            redact_url("ssh://git:secret@host.example:2222/repo@v2.git"),
            "ssh://git@host.example:2222/repo@v2.git"
        );
        assert_eq!(
            redact_url("git+ssh://deploy:p@ss:word@host.example/x"),
            "git+ssh://deploy@host.example/x"
        );
        assert_eq!(
            redact_url("git://bob:secret@host.example/x"),
            "git://bob@host.example/x"
        );
        assert_eq!(
            redact_url("ftp://:secret@host.example/x"),
            "ftp://host.example/x"
        );
        assert_eq!(
            redact_url("HTTPS://alice:secret@host.example/x"),
            "HTTPS://host.example/x"
        );
    }

    #[test]
    fn leaves_urls_without_a_password_alone() {
        assert_eq!(
            redact_url("ssh://git@host.example:2222/x"),
            "ssh://git@host.example:2222/x"
        );
        assert_eq!(
            redact_url("ssh://host.example/path:with@signs"),
            "ssh://host.example/path:with@signs"
        );
        assert_eq!(
            redact_url("git@host.example:team/repo@v2.git"),
            "git@host.example:team/repo@v2.git"
        );
        assert_eq!(redact_url("/srv/git/a://b:c@d"), "/srv/git/a://b:c@d");
    }

    #[test]
    fn redaction_ignores_at_signs_outside_the_authority() {
        assert_eq!(
            redact_url("https://host.example/path@x"),
            "https://host.example/path@x"
        );
        assert_eq!(
            redact_url("https://host.example/repo?ref=a@b"),
            "https://host.example/repo?ref=a@b"
        );
        assert_eq!(
            redact_url("https://host.example?ref=a@b"),
            "https://host.example?ref=a@b"
        );
        assert_eq!(
            redact_url("http://host.example#frag@x"),
            "http://host.example#frag@x"
        );
        assert_eq!(
            redact_url("https://alice:secret@host.example/team@org/repo.git"),
            "https://host.example/team@org/repo.git"
        );
    }

    #[test]
    fn private_key_candidates_are_id_files_without_pub_extension() {
        assert!(is_private_key_candidate(Path::new("/h/.ssh/id_ed25519")));
        assert!(is_private_key_candidate(Path::new("/h/.ssh/id_work")));
        assert!(is_private_key_candidate(Path::new("/h/.ssh/id_work.pem")));
        assert!(!is_private_key_candidate(Path::new("/h/.ssh/id_work.pub")));
        assert!(!is_private_key_candidate(Path::new("/h/.ssh/id_work.PUB")));
        assert!(!is_private_key_candidate(Path::new("/h/.ssh/known_hosts")));
        assert!(!is_private_key_candidate(Path::new("/h/.ssh/config")));
    }

    fn drain(
        sequence: &mut AuthSequence,
        allowed: git2::CredentialType,
        keys: &[PathBuf],
        env_token_set: bool,
    ) -> Vec<AuthAttempt> {
        std::iter::from_fn(|| sequence.next(allowed, keys, env_token_set)).collect()
    }

    #[test]
    fn ssh_sequence_tries_the_agent_then_every_key_then_stops() {
        let keys = [
            PathBuf::from("/h/.ssh/id_ed25519"),
            "/h/.ssh/id_work".into(),
        ];
        let mut sequence = AuthSequence::default();
        assert_eq!(
            drain(&mut sequence, git2::CredentialType::SSH_KEY, &keys, true),
            vec![
                AuthAttempt::Agent,
                AuthAttempt::KeyFile(keys[0].clone()),
                AuthAttempt::KeyFile(keys[1].clone()),
            ]
        );
        assert_eq!(
            sequence.next(git2::CredentialType::SSH_KEY, &keys, true),
            None
        );
    }

    #[test]
    fn https_sequence_tries_the_env_token_before_the_helper() {
        let allowed = git2::CredentialType::USER_PASS_PLAINTEXT;
        assert_eq!(
            drain(&mut AuthSequence::default(), allowed, &[], true),
            vec![AuthAttempt::EnvToken, AuthAttempt::CredentialHelper]
        );
        assert_eq!(
            drain(&mut AuthSequence::default(), allowed, &[], false),
            vec![AuthAttempt::CredentialHelper]
        );
    }

    #[test]
    fn auth_plan_keeps_the_documented_order_across_types() {
        let keys = [PathBuf::from("/h/.ssh/id_rsa")];
        let allowed = git2::CredentialType::SSH_KEY
            | git2::CredentialType::USER_PASS_PLAINTEXT
            | git2::CredentialType::DEFAULT;
        assert_eq!(
            auth_plan(allowed, &keys, true),
            vec![
                AuthAttempt::Agent,
                AuthAttempt::KeyFile(keys[0].clone()),
                AuthAttempt::EnvToken,
                AuthAttempt::CredentialHelper,
                AuthAttempt::Default,
            ]
        );
    }

    #[test]
    fn auth_sequence_offers_nothing_for_unsupported_types() {
        let mut sequence = AuthSequence::default();
        let keys = [PathBuf::from("/h/.ssh/id_rsa")];
        assert_eq!(
            sequence.next(git2::CredentialType::SSH_CUSTOM, &keys, true),
            None
        );
        assert!(sequence.offered.is_empty());
    }

    #[test]
    fn auth_sequence_does_not_repeat_when_allowed_types_change() {
        let keys = [PathBuf::from("/h/.ssh/id_rsa")];
        let mut sequence = AuthSequence::default();
        let ssh = git2::CredentialType::SSH_KEY;
        let both = ssh | git2::CredentialType::USER_PASS_PLAINTEXT;
        assert_eq!(sequence.next(ssh, &keys, false), Some(AuthAttempt::Agent));
        assert_eq!(
            drain(&mut sequence, both, &keys, false),
            vec![
                AuthAttempt::KeyFile(keys[0].clone()),
                AuthAttempt::CredentialHelper
            ]
        );
    }

    #[test]
    fn private_key_files_lists_standard_names_first_without_duplicates() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        for name in [
            "id_work",
            "id_rsa",
            "id_ed25519",
            "id_ed25519.pub",
            "id_alpha",
            "known_hosts",
        ] {
            fs::write(tmp.path().join(name), "x").expect("write");
        }
        fs::create_dir(tmp.path().join("id_dir")).expect("mkdir");
        let names: Vec<String> = private_key_files(tmp.path())
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["id_ed25519", "id_rsa", "id_alpha", "id_work"]);
    }

    #[test]
    fn private_key_files_is_empty_without_an_ssh_dir() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        assert!(private_key_files(&tmp.path().join("missing")).is_empty());
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

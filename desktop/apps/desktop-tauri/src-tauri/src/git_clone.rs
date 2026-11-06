//! `project/clone`: clone a remote repository into a folder the owner picked.
//! The renderer validates the URL for its own UI, but this is the privileged
//! side, so validation is repeated here (a renderer is never trusted) and git
//! runs without a shell or a terminal prompt.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use url::Url;

/// Long enough for a large repository over a slow link; a hung transfer must
/// still end instead of holding the invoke forever.
const TIMEOUT: Duration = Duration::from_secs(600);
const MAX_URL_LEN: usize = 2048;
const MAX_NAME_LEN: usize = 120;

pub enum CloneFailure {
    /// The URL cannot be cloned safely; the renderer gets INVALID_ARGUMENT.
    InvalidUrl(String),
    /// git started and failed, or could not start; INTERNAL.
    Git(String),
}

/// The folder a URL asks for, or `None` when it cannot name one.
pub fn repo_name(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() || url.len() > MAX_URL_LEN || url.chars().any(char::is_whitespace) {
        return None;
    }
    // `git@host:org/repo.git` never parses as a URL.
    if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        return host_is_public(host).then(|| name_from_path(path)).flatten();
    }
    let parsed = Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "https" | "http" | "ssh" | "git") {
        return None;
    }
    // Credentials stay out of the URL; git asks for them itself when needed.
    if parsed.password().is_some() {
        return None;
    }
    host_is_public(parsed.host_str()?)
        .then(|| name_from_path(parsed.path()))
        .flatten()
}

fn name_from_path(path: &str) -> Option<String> {
    let segment = path
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())?;
    let lower = segment.to_ascii_lowercase();
    let name = if lower.ends_with(".git") {
        &segment[..segment.len() - 4]
    } else {
        segment
    };
    if !valid_name(name) {
        return None;
    }
    Some(name.to_string())
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let starts_ok = chars
        .next()
        .map(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        .unwrap_or(false);
    starts_ok
        && name.len() <= MAX_NAME_LEN
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// A clone host must be a name or a public IP: loopback and private ranges
/// would let a pasted URL reach a service this machine only exposes locally.
pub(crate) fn host_is_public(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']);
    if host.eq_ignore_ascii_case("localhost") {
        return false;
    }
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return !(ip.is_loopback()
            || ip.is_private()
            || ip.is_link_local()
            || ip.is_unspecified()
            || ip.is_broadcast()
            || ip.octets()[0] == 0);
    }
    if let Ok(ip) = host.parse::<Ipv6Addr>() {
        return !(ip.is_loopback() || ip.is_unspecified() || ip.is_unique_local());
    }
    !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

pub async fn clone(url: &str, parent: &Path) -> Result<PathBuf, CloneFailure> {
    let name = repo_name(url).ok_or_else(|| {
        CloneFailure::InvalidUrl(
            "url must be a public https/http/ssh/git repository that names a folder".into(),
        )
    })?;
    let dest = parent.join(&name);
    if dest.exists() {
        return Err(CloneFailure::InvalidUrl(format!(
            "{name} already exists in the selected folder"
        )));
    }

    let output = tokio::time::timeout(
        TIMEOUT,
        tokio::process::Command::new("git")
            .arg("clone")
            .arg("--")
            .arg(url)
            .arg(&dest)
            // An auth prompt would hang a window-less process.
            .env("GIT_TERMINAL_PROMPT", "0")
            .kill_on_drop(true)
            .output(),
    )
    .await;

    let output = match output {
        Err(_) => {
            remove_partial(&dest);
            return Err(CloneFailure::Git("git clone timed out".into()));
        }
        Ok(Err(error)) => {
            remove_partial(&dest);
            return Err(CloneFailure::Git(format!("could not run git: {error}")));
        }
        Ok(Ok(output)) => output,
    };

    if output.status.success() {
        return Ok(dest);
    }
    remove_partial(&dest);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("git clone failed");
    Err(CloneFailure::Git(detail.to_string()))
}

/// The destination did not exist before this clone, so a failed attempt only
/// leaves git's own partial checkout behind.
fn remove_partial(dest: &Path) {
    if dest.exists() {
        let _ = std::fs::remove_dir_all(dest);
    }
}

/// Best-effort branch for the workspace record, read like `workspace.get`.
pub fn head_branch(path: &Path) -> Option<String> {
    let head = std::fs::read_to_string(path.join(".git").join("HEAD")).ok()?;
    head.lines().find_map(|line| {
        line.strip_prefix("ref: refs/heads/")
            .map(|b| b.trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::{host_is_public, repo_name};

    #[test]
    fn names_follow_the_remote_path() {
        assert_eq!(
            repo_name("https://github.com/org/repo.git").as_deref(),
            Some("repo")
        );
        assert_eq!(
            repo_name("https://github.com/org/repo/").as_deref(),
            Some("repo")
        );
        assert_eq!(
            repo_name("git@github.com:org/repo.git").as_deref(),
            Some("repo")
        );
        assert_eq!(
            repo_name("ssh://git@example.com:22/org/repo").as_deref(),
            Some("repo")
        );
        assert_eq!(
            repo_name("https://example.com/org/repo.GIT").as_deref(),
            Some("repo")
        );
    }

    #[test]
    fn credentials_and_unsafe_names_are_rejected() {
        assert!(repo_name("https://user:secret@example.com/org/repo").is_none());
        assert!(repo_name("file:///tmp/repo").is_none());
        assert!(repo_name("https://example.com/org/..").is_none());
        assert!(repo_name("https://example.com/org/a b").is_none());
    }

    #[test]
    fn local_hosts_are_not_clone_targets() {
        assert!(!host_is_public("localhost"));
        assert!(!host_is_public("127.0.0.1"));
        assert!(!host_is_public("192.168.1.10"));
        assert!(!host_is_public("10.0.0.2"));
        assert!(!host_is_public("[::1]"));
        assert!(host_is_public("github.com"));
        assert!(repo_name("https://127.0.0.1/org/repo").is_none());
    }
}

//! Focus detection — /proc process tree walk and session matching.
//!
//! Resolves which clippy session, if any, has X11 focus by walking
//! the process tree from each session's child PID upward to find the
//! window-owning process. See CONTRACT_HOTKEY.md §94–128.

use crate::ipc::protocol::SessionDescriptor;

/// Focus resolution error.
#[derive(Debug)]
pub enum FocusError {
    /// No registered clippy session matches the focused window.
    NoSession,
    /// Multiple sessions match — ambiguous (e.g., split panes in
    /// the same terminal emulator).
    Ambiguous(Vec<String>),
}

impl std::fmt::Display for FocusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSession => write!(f, "no clippy session in focused window"),
            Self::Ambiguous(ids) => {
                write!(f, "ambiguous — multiple sessions match: {}", ids.join(", "))
            }
        }
    }
}

/// Get the parent PID of a process by reading `/proc/{pid}/status`.
///
/// Returns `None` if the process doesn't exist, `/proc` is unavailable,
/// or the status file doesn't contain a valid `PPid` line.
pub fn get_ppid(pid: u32) -> Option<u32> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in status.lines() {
        if let Some(ppid_str) = line.strip_prefix("PPid:") {
            return ppid_str.trim().parse().ok();
        }
    }
    None
}

/// Check if `ancestor_pid` is an ancestor of `descendant_pid` in the
/// process tree.
///
/// Walks upward from `descendant_pid` via `/proc/{pid}/status` `PPid`
/// until finding `ancestor_pid` (returns `true`) or reaching PID 0/1
/// (returns `false`).
///
/// Returns `false` if `ancestor_pid == descendant_pid` — we're looking
/// for strict ancestry (the window PID must be a parent/grandparent of
/// the session's child PID, not the child itself).
pub fn is_ancestor(ancestor_pid: u32, descendant_pid: u32) -> bool {
    let mut current = descendant_pid;
    // Guard against cycles — limit walk depth.
    for _ in 0..1024 {
        match get_ppid(current) {
            Some(ppid) if ppid == ancestor_pid => return true,
            Some(ppid) if ppid > 1 && ppid != current => current = ppid,
            _ => return false,
        }
    }
    false
}

/// Resolve which broker session, if any, is owned by the focused
/// window's process.
///
/// Per CONTRACT_HOTKEY.md §104–114:
/// - Walk the process tree from each session's child PID upward.
/// - If the window PID is an ancestor: the session is a candidate.
/// - Exactly one match → return that session ID.
/// - Zero matches → `FocusError::NoSession`.
/// - Multiple matches → `FocusError::Ambiguous`.
pub fn resolve_session(
    window_pid: u32,
    sessions: &[SessionDescriptor],
) -> Result<String, FocusError> {
    let mut matches = Vec::new();

    for session in sessions {
        if session.pid == window_pid || is_ancestor(window_pid, session.pid) {
            matches.push(session.session.clone());
        }
    }

    match matches.len() {
        0 => Err(FocusError::NoSession),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(FocusError::Ambiguous(matches)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_ppid_of_self() {
        let my_pid = std::process::id();
        let ppid = get_ppid(my_pid);
        // Our parent must exist and be > 0.
        assert!(ppid.is_some(), "should be able to read own PPid");
        assert!(ppid.unwrap() > 0, "PPid should be > 0");
    }

    #[test]
    fn get_ppid_of_nonexistent_pid() {
        // PID 4294967295 is extremely unlikely to exist.
        assert_eq!(get_ppid(u32::MAX), None);
    }

    #[test]
    fn get_ppid_of_init() {
        // PID 1 (init) should have PPid 0.
        let ppid = get_ppid(1);
        assert_eq!(ppid, Some(0));
    }

    #[test]
    fn is_ancestor_parent_of_self() {
        let my_pid = std::process::id();
        let my_ppid = get_ppid(my_pid).expect("should have PPid");
        assert!(is_ancestor(my_ppid, my_pid), "parent should be an ancestor");
    }

    #[test]
    fn is_ancestor_init_is_ancestor_of_self() {
        let my_pid = std::process::id();
        // PID 1 (init) is an ancestor of every process.
        assert!(
            is_ancestor(1, my_pid),
            "init (PID 1) should be an ancestor of any process"
        );
    }

    #[test]
    fn is_ancestor_self_is_not_own_ancestor() {
        let my_pid = std::process::id();
        // A process is not a strict ancestor of itself.
        assert!(
            !is_ancestor(my_pid, my_pid),
            "a process should not be its own ancestor"
        );
    }

    #[test]
    fn is_ancestor_nonexistent_returns_false() {
        assert!(!is_ancestor(u32::MAX, std::process::id()));
    }

    #[test]
    fn resolve_session_single_match() {
        let my_pid = std::process::id();
        let my_ppid = get_ppid(my_pid).unwrap();

        let sessions = vec![SessionDescriptor {
            session: "s1".into(),
            pid: my_pid,
            has_turn: false,
        }];

        // Our parent should be an ancestor of our PID.
        let result = resolve_session(my_ppid, &sessions);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "s1");
    }

    #[test]
    fn resolve_session_no_match() {
        let sessions = vec![SessionDescriptor {
            session: "s1".into(),
            pid: 1, // init — window PID 999999 is not an ancestor of PID 1
            has_turn: false,
        }];

        let result = resolve_session(999_999, &sessions);
        assert!(matches!(result, Err(FocusError::NoSession)));
    }

    #[test]
    fn resolve_session_ambiguous() {
        let my_pid = std::process::id();
        let my_ppid = get_ppid(my_pid).unwrap();

        // Two sessions with the same PID — both match.
        let sessions = vec![
            SessionDescriptor {
                session: "s1".into(),
                pid: my_pid,
                has_turn: false,
            },
            SessionDescriptor {
                session: "s2".into(),
                pid: my_pid,
                has_turn: true,
            },
        ];

        let result = resolve_session(my_ppid, &sessions);
        assert!(matches!(result, Err(FocusError::Ambiguous(ref ids)) if ids.len() == 2));
    }

    #[test]
    fn resolve_session_empty_list() {
        let result = resolve_session(1, &[]);
        assert!(matches!(result, Err(FocusError::NoSession)));
    }

    #[test]
    fn resolve_session_direct_pid_match() {
        // If window PID == session PID, it should match.
        let my_pid = std::process::id();
        let sessions = vec![SessionDescriptor {
            session: "s1".into(),
            pid: my_pid,
            has_turn: false,
        }];

        let result = resolve_session(my_pid, &sessions);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "s1");
    }
}

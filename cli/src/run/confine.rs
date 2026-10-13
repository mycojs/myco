//! Path confinement shared by module resolution and directory capabilities.
//!
//! Follow symlinks before applying `..`; allow missing components for writes,
//! but refuse paths that cannot be inspected. `canonicalize` alone cannot
//! answer for missing targets or expanded paths beyond `PATH_MAX`.
//!
//! This checks names, not file identity: hardlinks and bind mounts are invisible,
//! and components can change between checking and opening. Atomic confinement
//! requires descriptor-relative access, such as `openat2` with `RESOLVE_BENEATH`.

use std::path::{Component, Path, PathBuf};

// Match Linux's symlink traversal limit and bound cycles on all platforms.
const MAX_HOPS: usize = 40;

/// Where a path ends up once everything on the way has been followed.
#[derive(Debug, PartialEq, Eq)]
pub enum Reach {
    /// The resolved target is within the roots.
    Inside,
    Outside,
    /// A cycle, excessive chain, or unreadable component.
    Unresolvable,
}

/// Directories a path may not leave.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    roots: Vec<PathBuf>,
}

impl Roots {
    /// Include canonical roots when granting access through symlinks.
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Roots { roots }
    }

    #[cfg(test)]
    pub fn single(root: PathBuf) -> Self {
        Roots { roots: vec![root] }
    }

    /// Lexical containment; callers must also check [`Roots::reach`].
    pub fn contains(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| path.starts_with(root))
    }

    /// Where the path lands once everything on the way has been followed.
    pub fn reach(&self, path: &Path) -> Reach {
        match resolve_through_links(path) {
            Some(landing) => {
                if self.contains(&landing) {
                    Reach::Inside
                } else {
                    Reach::Outside
                }
            }
            None => Reach::Unresolvable,
        }
    }
}

/// Walk left to right, expanding links before applying `..`.
/// Missing components are allowed so writes can create their targets.
pub(super) fn resolve_through_links(path: &Path) -> Option<PathBuf> {
    resolve_links(path, None).ok()
}

fn resolve_links(path: &Path, boundary: Option<&Path>) -> Result<PathBuf, Refusal> {
    enum Step {
        Prefix(std::ffi::OsString),
        Root(std::ffi::OsString),
        Up,
        Name(std::ffi::OsString),
    }

    fn steps(path: &Path) -> Vec<Step> {
        path.components()
            .filter_map(|component| match component {
                Component::CurDir => None,
                Component::Prefix(prefix) => Some(Step::Prefix(prefix.as_os_str().to_os_string())),
                Component::RootDir => {
                    Some(Step::Root(Component::RootDir.as_os_str().to_os_string()))
                }
                Component::ParentDir => Some(Step::Up),
                Component::Normal(name) => Some(Step::Name(name.to_os_string())),
            })
            .collect()
    }

    let mut landing = PathBuf::new();
    // Reverse the work stack so `pop` consumes the next component.
    let mut pending: Vec<Step> = steps(path);
    pending.reverse();
    let mut hops = 0;

    while let Some(step) = pending.pop() {
        match step {
            Step::Prefix(prefix) => landing = PathBuf::from(prefix),
            // Pushing a root preserves a Windows drive/UNC prefix.
            Step::Root(root) => landing.push(root),
            Step::Up => {
                if boundary == Some(landing.as_path()) {
                    return Err(Refusal::Outside);
                }
                landing.pop();
            }
            Step::Name(name) => {
                let candidate = landing.join(name);
                let target = match std::fs::read_link(&candidate) {
                    Ok(target) => target,
                    // Not a link, absent, or unreachable through a regular file.
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::InvalidInput
                                | std::io::ErrorKind::NotFound
                                | std::io::ErrorKind::NotADirectory
                        ) || (cfg!(windows) && e.raw_os_error() == Some(4390)) =>
                    {
                        // Windows reports ERROR_NOT_A_REPARSE_POINT (4390) for
                        // ordinary files/directories; Rust leaves it Uncategorized.
                        landing = candidate;
                        continue;
                    }
                    // In particular, ENAMETOOLONG does not prove absence: the
                    // kernel may still open the caller's shorter symlinked path.
                    Err(_) => return Err(Refusal::Unresolvable),
                };
                hops += 1;
                if hops > MAX_HOPS {
                    return Err(Refusal::Unresolvable);
                }
                #[cfg(windows)]
                let target = windows_link_target(target, path);
                if target.is_absolute() {
                    landing = PathBuf::new();
                }
                let mut expanded = steps(&target);
                expanded.reverse();
                pending.extend(expanded);
            }
        }
    }

    Ok(landing)
}

// read_link returns ordinary drive/UNC paths, while canonical grants use verbatim
// prefixes. Keep absolute targets in the input path's namespace without collapsing `..`.
#[cfg(windows)]
fn windows_link_target(target: PathBuf, source: &Path) -> PathBuf {
    use std::ffi::OsString;
    use std::path::Prefix;

    if !target.is_absolute()
        || !matches!(source.components().next(), Some(Component::Prefix(p)) if p.kind().is_verbatim())
    {
        return target;
    }
    let Some(Component::Prefix(prefix)) = target.components().next() else {
        return target;
    };
    let mut native = match prefix.kind() {
        Prefix::Disk(_) => {
            let mut native = OsString::from(r"\\?\");
            native.push(prefix.as_os_str());
            native
        }
        Prefix::UNC(server, share) => {
            let mut native = OsString::from(r"\\?\UNC\");
            native.push(server);
            native.push(r"\");
            native.push(share);
            native
        }
        _ => return target,
    };
    let rest: PathBuf = target.components().skip(1).collect();
    native.push(rest.as_os_str());
    PathBuf::from(native)
}

/// What the operation does with a symlink it finds at the end of the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalLink {
    /// Read, write, exec, stat, and list follow the target.
    Followed,
    /// Removal acts on the link itself, even if its target is outside or absent.
    Unlinked,
}

/// Why a path was refused, without exposing destinations outside the grant.
#[derive(Debug)]
pub enum Refusal {
    Outside,
    Unresolvable,
}

/// Resolve `path` against `root`, refusing anything that leaves it.
///
/// Return the resolved path, preserving the final name for unlinking.
pub fn confine(root: &Path, path: &str, final_link: FinalLink) -> Result<PathBuf, Refusal> {
    let resolved = join_within(root, path)?;

    // Only a final name can be unlinked. A trailing separator, `.` or `..`
    // requires following the preceding component, even for removal.
    let last = path.rsplit(std::path::is_separator).next().unwrap_or("");
    let unlink_leaf = final_link == FinalLink::Unlinked && !matches!(last, "" | "." | "..");
    let probe = if unlink_leaf {
        resolved.parent().unwrap_or(root)
    } else {
        resolved.as_path()
    };

    let landing = resolve_links(probe, Some(root))?;
    if landing.starts_with(root) {
        if unlink_leaf {
            Ok(landing.join(resolved.file_name().expect("unlinking a final name")))
        } else if final_link == FinalLink::Unlinked {
            // Keep directory-only suffixes: rmdir("link/.") must not turn into
            // rmdir(target), which could delete an otherwise untouched directory.
            Ok(resolved)
        } else {
            Ok(landing)
        }
    } else {
        Err(Refusal::Outside)
    }
}

/// Anchor a caller's path without interpreting `..` before following symlinks.
fn join_within(root: &Path, path: &str) -> Result<PathBuf, Refusal> {
    // A leading `/` names the root of the grant, not the root of the filesystem.
    let relative = path.trim_start_matches('/');
    #[cfg(windows)]
    let relative = relative.replace('/', r"\");
    let relative = Path::new(&relative);

    // A Windows prefix must not replace the granted root.
    if relative
        .components()
        .any(|component| matches!(component, Component::RootDir | Component::Prefix(_)))
    {
        return Err(Refusal::Outside);
    }
    // PathBuf::join normalizes `..` under Windows verbatim roots. Append to the
    // native string instead, retaining the traversal the walker must interpret.
    let mut resolved = root.join("").into_os_string();
    resolved.push(relative.as_os_str());
    Ok(PathBuf::from(resolved))
}

/// Collapse `.` and `..` without touching the filesystem, so containment can be
/// settled before anything is observed.
pub fn lexically_normalize(path: &Path) -> PathBuf {
    let mut components: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match components.last() {
                Some(Component::Normal(_)) => {
                    components.pop();
                }
                // `/..` is `/`.
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => components.push(component),
            },
            _ => components.push(component),
        }
    }
    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_without_touching_the_filesystem() {
        assert_eq!(
            lexically_normalize(Path::new("/proj/src/../lib/./index.ts")),
            PathBuf::from("/proj/lib/index.ts")
        );
        // No climbing past the root.
        assert_eq!(
            lexically_normalize(Path::new("/proj/../../../etc/passwd")),
            PathBuf::from("/etc/passwd")
        );
        // A relative path keeps its leading `..`.
        assert_eq!(
            lexically_normalize(Path::new("../../a/b")),
            PathBuf::from("../../a/b")
        );
        assert_eq!(
            lexically_normalize(Path::new("a/../../b")),
            PathBuf::from("../b")
        );
    }

    #[test]
    fn resolution_without_links_is_lexical() {
        let scratch = ScratchDir::new("no-links");
        let root = scratch.root();

        for (path, expected) in [
            ("sub", root.join("sub")),
            ("sub/../absent", root.join("absent")),
            ("./sub/./x", root.join("sub/x")),
            ("sub/deeper/../../x", root.join("x")),
        ] {
            assert_eq!(
                resolve_through_links(&root.join(path)),
                Some(expected),
                "{path}"
            );
        }
    }

    #[test]
    fn caller_paths_stay_beneath_the_grant_root() {
        let scratch = ScratchDir::new("relative");
        let root = scratch.root();
        for (path, expected) in [
            ("file.txt", root.join("file.txt")),
            ("/file.txt", root.join("file.txt")),
            ("/", root.clone()),
            ("sub/..", root.clone()),
            ("sub/../file.txt", root.join("file.txt")),
            (
                "missing/deeper/new.txt",
                root.join("missing/deeper/new.txt"),
            ),
        ] {
            assert_eq!(confine(&root, path, FinalLink::Followed).unwrap(), expected);
        }
        for path in ["..", "../secret", "/../secret", "sub/../../secret"] {
            assert!(matches!(
                confine(&root, path, FinalLink::Followed),
                Err(Refusal::Outside)
            ));
        }
        let back_inside = format!(
            "../{}/file.txt",
            root.file_name().unwrap().to_string_lossy()
        );
        assert!(matches!(
            confine(&root, &back_inside, FinalLink::Followed),
            Err(Refusal::Outside)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn absolute_paths_preserve_the_windows_volume() {
        let scratch = ScratchDir::new("windows-absolute");
        let root = scratch.root();
        assert_eq!(
            resolve_through_links(&root.join("missing")),
            Some(root.join("missing"))
        );
        std::fs::write(root.join("file"), "contents").unwrap();
        assert_eq!(
            confine(&root, "sub/../file", FinalLink::Followed).unwrap(),
            root.join("file")
        );
        for path in [
            r"C:\outside",
            r"C:outside",
            r"\outside",
            r"\\server\share\outside",
        ] {
            assert!(matches!(
                confine(&root, path, FinalLink::Followed),
                Err(Refusal::Outside)
            ));
        }
    }

    #[cfg(windows)]
    #[test]
    fn absolute_link_targets_keep_the_canonical_namespace_and_parent_steps() {
        for (source, target, expected) in [
            (
                r"\\?\C:\root",
                r"C:\root\alias\..\file",
                r"\\?\C:\root\alias\..\file",
            ),
            (
                r"\\?\UNC\server\share\root",
                r"\\server\share\root\alias\..\file",
                r"\\?\UNC\server\share\root\alias\..\file",
            ),
        ] {
            assert_eq!(
                windows_link_target(PathBuf::from(target), Path::new(source)),
                PathBuf::from(expected)
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn admits_live_chains_up_to_the_kernel_limit() {
        let scratch = ScratchDir::new("long-chain");
        let root = scratch.root();
        std::fs::write(root.join("end"), "contents").unwrap();
        for i in (0..MAX_HOPS).rev() {
            let target = if i + 1 == MAX_HOPS {
                "end".to_string()
            } else {
                format!("link{}", i + 1)
            };
            link(&target, root.join(format!("link{i}")));
        }
        assert_eq!(
            std::fs::read_to_string(root.join("link0")).unwrap(),
            "contents"
        );
        assert_eq!(scratch.roots().reach(&root.join("link0")), Reach::Inside);
        link("link0", root.join("over-limit"));
        assert_eq!(
            scratch.roots().reach(&root.join("over-limit")),
            Reach::Unresolvable
        );
    }

    #[cfg(unix)]
    #[test]
    fn parent_traversal_pops_what_a_link_resolved_to() {
        let scratch = ScratchDir::new("dotdot-link");
        let root = scratch.root();
        std::fs::create_dir(scratch.path.join("outside/deep")).unwrap();

        link("../outside/deep", root.join("a"));
        link("sub", root.join("inward"));

        // `a` resolves out of the root, so `a/..` is outside it, not the root.
        assert_eq!(
            resolve_through_links(&root.join("a/../PLANTED.txt")),
            Some(scratch.path.join("outside/PLANTED.txt"))
        );
        // The same arithmetic when the link stays inside.
        assert_eq!(
            resolve_through_links(&root.join("inward/../x")),
            Some(root.join("x"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn caller_parent_traversal_writes_and_unlinks_the_os_target() {
        let scratch = ScratchDir::new("caller-dotdot");
        let root = scratch.root();
        std::fs::create_dir(root.join("sub/deep")).unwrap();
        std::fs::write(root.join("x"), "root").unwrap();
        std::fs::write(root.join("sub/x"), "sub").unwrap();
        link("sub/deep", root.join("inward"));
        link("../outside", root.join("outward"));
        link("../../outside/secret.txt", root.join("sub/leaf"));

        let requested = root.join("inward/../x");
        let checked = confine(&root, "inward/../x", FinalLink::Followed).unwrap();
        assert_eq!(checked, requested.canonicalize().unwrap());
        std::fs::write(checked, "changed").unwrap();
        assert_eq!(std::fs::read_to_string(root.join("x")).unwrap(), "root");
        assert_eq!(
            std::fs::read_to_string(root.join("sub/x")).unwrap(),
            "changed"
        );
        assert_eq!(
            confine(&root, "inward/../../x", FinalLink::Followed).unwrap(),
            root.join("x")
        );

        std::fs::remove_file(confine(&root, "inward/../leaf", FinalLink::Unlinked).unwrap())
            .unwrap();
        assert!(!root.join("sub/leaf").is_symlink());
        assert_eq!(
            std::fs::read_to_string(scratch.path.join("outside/secret.txt")).unwrap(),
            "secret"
        );
        for path in ["outward/", "outward/.", "outward/..", "outward/secret.txt"] {
            assert!(
                matches!(
                    confine(&root, path, FinalLink::Unlinked),
                    Err(Refusal::Outside)
                ),
                "{path}"
            );
        }
        for path in ["inward/", "inward/."] {
            let checked = confine(&root, path, FinalLink::Unlinked).unwrap();
            assert!(std::fs::remove_dir(checked).is_err(), "{path}");
            assert!(root.join("sub/deep").is_dir());
            assert!(root.join("inward").is_symlink());
        }
    }

    #[test]
    fn containment_is_by_component_not_by_prefix_string() {
        let roots = Roots::single(PathBuf::from("/proj"));

        assert!(roots.contains(Path::new("/proj")));
        assert!(roots.contains(Path::new("/proj/src/index.ts")));
        // A name that merely starts with the root's is not inside it.
        assert!(!roots.contains(Path::new("/project-other/evil.ts")));
        assert!(!roots.contains(Path::new("/etc/passwd")));
    }

    #[test]
    fn empty_roots_contain_nothing() {
        assert!(!Roots::default().contains(Path::new("/anything")));
    }

    struct ScratchDir {
        path: PathBuf,
    }

    impl ScratchDir {
        fn new(name: &str) -> ScratchDir {
            let path =
                std::env::temp_dir().join(format!("myco-confine-{}-{}", std::process::id(), name));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(path.join("root/sub")).unwrap();
            std::fs::create_dir_all(path.join("outside")).unwrap();
            std::fs::write(path.join("outside/secret.txt"), "secret").unwrap();
            ScratchDir {
                path: path.canonicalize().unwrap(),
            }
        }

        fn root(&self) -> PathBuf {
            self.path.join("root")
        }

        fn roots(&self) -> Roots {
            Roots::single(self.root())
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[cfg(unix)]
    fn link(original: &str, at: PathBuf) {
        std::os::unix::fs::symlink(original, at).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn reach_answers_where_a_path_lands() {
        let scratch = ScratchDir::new("reach");
        let root = scratch.root();
        let roots = scratch.roots();

        link("nowhere.txt", root.join("dangling"));
        link("dangling", root.join("chain"));
        link("sub", root.join("live"));
        link("../outside", root.join("escape"));
        link("cycle_b", root.join("cycle_a"));
        link("cycle_a", root.join("cycle_b"));
        // The target spells a path inside the root, but reaches it through a
        // link that leads out.
        link("escape/planted.txt", root.join("via_escape"));
        link("via_escape", root.join("via_escape_twice"));
        // The same, with the escaping component erased by a `..` rather than
        // followed by more of the path. Collapsing that `..` lexically reads
        // this as `root/planted.txt`.
        std::fs::create_dir(scratch.path.join("outside/deep")).unwrap();
        link("../outside/deep", root.join("deep_escape"));
        link("deep_escape/../planted.txt", root.join("via_dotdot"));

        let inside = [
            "sub",            // a plain directory
            "absent.txt",     // nothing there, and no link on the way
            "sub/absent.txt", // nor deeper
            "dangling",       // a link into the root whose target is absent
            "chain",          // and one reached through another link
            "live",           // a link that resolves inside
            "live/file.txt",  // and something under it that does not exist yet
        ];
        for path in inside {
            assert_eq!(roots.reach(&root.join(path)), Reach::Inside, "{path}");
        }

        let outside = [
            "escape",            // the link itself
            "escape/secret.txt", // something real beyond it
            "escape/absent.txt", // and something that is not there
            "via_escape",        // target lexically inside, lands outside
            "via_escape_twice",  // the same, one level of indirection further
            "via_dotdot",        // the escaping component erased by a `..`
            "deep_escape/../x",  // and the same shape written out directly
        ];
        for path in outside {
            assert!(
                matches!(roots.reach(&root.join(path)), Reach::Outside),
                "{path} was not reported as outside: {:?}",
                roots.reach(&root.join(path))
            );
        }

        // A cycle terminates, and is not an escape.
        assert!(matches!(
            roots.reach(&root.join("cycle_a")),
            Reach::Unresolvable
        ));
    }

    // Only the link beyond PATH_MAX escapes; the earlier components stay inside.
    #[cfg(unix)]
    #[test]
    fn refuses_a_path_too_long_to_examine() {
        const PATH_MAX: usize = 4096;
        let scratch = ScratchDir::new("toolong");
        let root = scratch.root();
        let outside = scratch.path.join("outside/PWNED.txt");
        std::fs::write(&outside, "original").unwrap();

        // Depth in two steps, so no path handed to the OS is itself over the
        // limit: a real tree that fits, then one more level through an alias.
        let seg = "d".repeat(200);
        let last = "e".repeat(200);
        let depth = (PATH_MAX - 6 - root.to_string_lossy().len()) / (seg.len() + 1);
        let mut deep = root.clone();
        for _ in 0..depth {
            deep.push(&seg);
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::os::unix::fs::symlink(deep.strip_prefix(&root).unwrap(), root.join("a0")).unwrap();
        std::fs::create_dir(root.join("a0").join(&last)).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("a0").join(&last).join("escape")).unwrap();
        std::os::unix::fs::symlink(PathBuf::from("a0").join(&last), root.join("a")).unwrap();

        // The fixture only means anything if the OS can still reach it.
        let candidate = root.join("a/escape");
        assert!(
            std::fs::write(&candidate, "reachable").is_ok(),
            "fixture: the OS must still open it"
        );
        assert!(
            candidate.canonicalize().is_err(),
            "fixture: canonicalize must fail"
        );

        assert!(
            matches!(
                confine(&root, "a/escape", FinalLink::Followed),
                Err(Refusal::Unresolvable)
            ),
            "a path we cannot examine was not refused"
        );
    }

    #[cfg(unix)]
    #[test]
    fn confine_resolves_followed_paths_but_preserves_the_unlinked_leaf() {
        let scratch = ScratchDir::new("confine");
        let root = scratch.root();

        link("../outside", root.join("escape"));
        link("nowhere.txt", root.join("dangling"));

        // Following uses the resolved target; unlinking keeps only the final link.
        assert_eq!(
            confine(&root, "dangling", FinalLink::Followed).unwrap(),
            root.join("nowhere.txt")
        );
        assert!(matches!(
            confine(&root, "escape/planted.txt", FinalLink::Followed),
            Err(Refusal::Outside)
        ));

        // Unlinking judges only what the link is reached through, so a link out
        // of the directory can be removed but not followed.
        assert_eq!(
            confine(&root, "escape", FinalLink::Unlinked).unwrap(),
            root.join("escape")
        );
        assert!(matches!(
            confine(&root, "escape", FinalLink::Followed),
            Err(Refusal::Outside)
        ));
        // Reaching *through* it is refused either way.
        assert!(matches!(
            confine(&root, "escape/secret.txt", FinalLink::Unlinked),
            Err(Refusal::Outside)
        ));

        // The root itself is in scope for both, and `..` at the root is not.
        assert_eq!(confine(&root, "/", FinalLink::Unlinked).unwrap(), root);
        assert_eq!(confine(&root, ".", FinalLink::Followed).unwrap(), root);
        assert!(matches!(
            confine(&root, "..", FinalLink::Unlinked),
            Err(Refusal::Outside)
        ));
    }
}

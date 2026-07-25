//! Deciding whether a path stays inside the directories a grant names.
//!
//! One decision, shared by the two places that make it: the module graph, bound
//! to the project ([`super::resolve`]), and directory tokens, bound to the
//! directory they were issued for ([`super::ops::filesystem`]). Those were
//! written separately, and each leak found in one of them was live in the other
//! -- #51, #61 and #63 are the same defect reported against different callers.
//!
//! Symlinks are the whole difficulty, and the rule is that a path is judged by
//! **where it lands**, not by whether it currently resolves and not by what it
//! spells. `canonicalize` answers only when every component exists, which the
//! common case of a write does not, and it fails past `PATH_MAX` while the
//! kernel still opens the file -- so a failure to resolve is evidence of
//! neither safety nor escape. Reasoning about the spelling instead is worse,
//! because `..` cannot be applied to a name that has not been followed. So
//! [`resolve_through_links`] walks the path as the kernel does, and a walk that
//! cannot be finished is [`Reach::Unresolvable`] rather than an escape.
//!
//! What this cannot see is anything that is not in the path. A hardlink is a
//! second name for the same file and `read_link` reports it as an ordinary one;
//! a bind mount is invisible for the same reason. No filesystem token can make
//! either -- there is no link operation -- but an exec token can run `ln`, so
//! this is not a bound on what a program can arrange, only on what it can
//! arrange without holding exec. There is also a window between the decision
//! here and the syscall that acts on it. Closing these means holding a
//! directory descriptor and using `openat` with `RESOLVE_BENEATH`, which is a
//! different design.

use std::path::{Component, Path, PathBuf};

/// A hop limit rather than a visited set: a cycle and a chain too long to walk
/// are refused the same way, and neither is worth the allocation to tell apart.
const MAX_HOPS: usize = 16;

/// Where a path ends up once everything on the way has been followed.
#[derive(Debug, PartialEq, Eq)]
pub enum Reach {
    /// Every link on the way stays within the roots.
    Inside,
    /// A link leads out, and this is where it lands.
    Outside(PathBuf),
    /// A path that could not be followed to an answer: a cycle or a chain
    /// longer than [`MAX_HOPS`], or a component we could not read at all --
    /// notably one past `PATH_MAX`, which the kernel would still resolve.
    /// Refused like [`Reach::Outside`], but it is a different fact, and callers
    /// that can afford to say so should: reporting a dangling link inside the
    /// directory as "outside the token's scope" sends whoever reads it looking
    /// for an escape that is not there.
    Unresolvable(PathBuf),
}

/// Directories a path may not leave.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    roots: Vec<PathBuf>,
}

impl Roots {
    /// The roots should be canonical. A grant that pins its root when it is
    /// made compares like with like here; one that stores what the caller typed
    /// is a grant to whatever that name resolves to at the time of the call.
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Roots { roots }
    }

    pub fn single(root: PathBuf) -> Self {
        Roots { roots: vec![root] }
    }

    /// Lexical containment only -- says nothing about what the path is reached
    /// through. [`Roots::reach`] is the other half, and both have to hold.
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
                    Reach::Outside(landing)
                }
            }
            None => Reach::Unresolvable(path.to_path_buf()),
        }
    }
}

/// Resolve a path the way the kernel will: left to right, following each link
/// as it is met, and applying `..` to what has actually been resolved so far.
///
/// The last component is allowed not to exist, and so is any component after a
/// link that dangles -- what is wanted is where the path *would* land, which is
/// what an operation that creates the missing part will use.
///
/// Doing this by manipulating the spelling of a path instead does not work, and
/// failed twice here before it was written this way. `canonicalize` gives up
/// entirely when any component is missing. Collapsing `..` lexically is worse
/// than giving up: `a/../x` under `a -> ../outside` reads as `x` in this
/// directory, having silently discarded the component that leaves it. A `..`
/// may only be applied once what precedes it is known, and the only way to know
/// it is to have followed it.
fn resolve_through_links(path: &Path) -> Option<PathBuf> {
    // Owned, because a link's target is walked by these same rules and does not
    // outlive the `read_link` that produced it.
    enum Step {
        Root(std::ffi::OsString),
        Up,
        Name(std::ffi::OsString),
    }

    fn steps(path: &Path) -> Vec<Step> {
        path.components()
            .filter_map(|component| match component {
                Component::CurDir => None,
                Component::Prefix(prefix) => Some(Step::Root(prefix.as_os_str().to_os_string())),
                Component::RootDir => {
                    Some(Step::Root(Component::RootDir.as_os_str().to_os_string()))
                }
                Component::ParentDir => Some(Step::Up),
                Component::Normal(name) => Some(Step::Name(name.to_os_string())),
            })
            .collect()
    }

    let mut landing = PathBuf::new();
    // Still to be consumed, innermost last so `pop` takes the next one.
    let mut pending: Vec<Step> = steps(path);
    pending.reverse();
    let mut hops = 0;

    while let Some(step) = pending.pop() {
        match step {
            Step::Root(root) => landing = PathBuf::from(root),
            // `landing` is resolved, so this pops what the kernel would pop.
            Step::Up => {
                landing.pop();
            }
            Step::Name(name) => {
                let candidate = landing.join(name);
                let target = match std::fs::read_link(&candidate) {
                    Ok(target) => target,
                    // The name is what it says: `EINVAL` is not a link at all,
                    // `ENOENT` is nothing there (the ordinary case for a write),
                    // and `ENOTDIR` is a path continuing through a file, which
                    // resolves to nothing however it is followed. None of the
                    // three can reach anywhere.
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::InvalidInput
                                | std::io::ErrorKind::NotFound
                                | std::io::ErrorKind::NotADirectory
                        ) =>
                    {
                        landing = candidate;
                        continue;
                    }
                    // Anything else means we could not find out -- and the one
                    // that matters is `ENAMETOOLONG`, since `landing` is the
                    // expanded path and grows while the caller's stays short.
                    // The kernel resolves incrementally and would still open it,
                    // so reading this as "not a link" hands back a path that
                    // walks straight out. Refuse instead.
                    Err(_) => return None,
                };
                hops += 1;
                if hops > MAX_HOPS {
                    return None;
                }
                if target.is_absolute() {
                    landing = PathBuf::new();
                }
                let mut expanded = steps(&target);
                expanded.reverse();
                pending.extend(expanded);
            }
        }
    }

    Some(landing)
}

/// What the operation does with a symlink it finds at the end of the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalLink {
    /// The operation follows it -- read, write, exec, stat, list. Where it
    /// points is what gets touched, so that has to be in scope.
    Followed,
    /// The operation acts on the link itself: `remove` unlinks the link rather
    /// than deleting what it points at. Nothing at the far end is touched, so a
    /// dangling or outward-pointing link inside the directory can still be
    /// cleaned up -- refusing that leaves a `WriteDir` holder unable to tidy
    /// its own directory (#61).
    Unlinked,
}

/// Why a path was refused. Both are refusals; they differ in what to say.
#[derive(Debug)]
pub enum Refusal {
    Outside(PathBuf),
    Unresolvable(PathBuf),
}

/// Resolve `path` against `root`, refusing anything that leaves it.
///
/// What comes back is the **lexical** path, with symlinks left unresolved, so
/// operations keep acting on what they were asked for. Every `..` has been
/// applied here rather than left for the OS, so the collapsed path is what gets
/// opened and a link followed at open time cannot be walked back out of.
pub fn confine(root: &Path, path: &str, final_link: FinalLink) -> Result<PathBuf, Refusal> {
    let resolved = join_within(root, path)?;
    let roots = Roots::single(root.to_path_buf());

    // Under `Unlinked` the last component is the thing being removed, not a
    // step on the way to it, so only what it is reached *through* is checked.
    // A path that collapsed back to the root has no last component to exempt,
    // and its parent is above the root, so the root itself is what to check.
    let probe = match final_link {
        FinalLink::Followed => resolved.as_path(),
        FinalLink::Unlinked if resolved == root => root,
        FinalLink::Unlinked => resolved.parent().unwrap_or(root),
    };

    match roots.reach(probe) {
        Reach::Inside => Ok(resolved),
        Reach::Outside(target) => Err(Refusal::Outside(target)),
        Reach::Unresolvable(link) => Err(Refusal::Unresolvable(link)),
    }
}

/// Build the path a component at a time, keeping it at or below `root`.
///
/// `..` is applied to what has been built rather than left in the path: one at
/// the root has nowhere to go and is refused, so no number of them walks out.
/// Leaving them in would defeat the check entirely, since `Path::starts_with`
/// compares components and does not interpret `..` (#51).
fn join_within(root: &Path, path: &str) -> Result<PathBuf, Refusal> {
    // A leading `/` names the root of the grant, not the root of the filesystem.
    let relative = PathBuf::from(path.trim_start_matches('/'));

    let mut resolved = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if resolved == root {
                    return Err(Refusal::Outside(resolved.join("..")));
                }
                resolved.pop();
            }
            // Unreachable on Unix now the leading `/` is trimmed, but a Windows
            // prefix (`C:\`) would otherwise replace the root wholesale.
            Component::RootDir | Component::Prefix(_) => {
                return Err(Refusal::Outside(PathBuf::from(path)));
            }
            Component::Normal(name) => resolved.push(name),
        }
    }
    Ok(resolved)
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

    // With no links involved this is ordinary path arithmetic, and `..` means
    // what it says.
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

    // `..` after a link is the case that cannot be answered lexically: it pops
    // where the link went, not the name that preceded it.
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

    // Everything below needs a real filesystem: symlinks are the point.
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

    // Every shape a path can take crossed with every state its far end can be
    // in. `reach` is what the confinement rests on, and each leak so far has
    // been one uncovered cell of this table.
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
                matches!(roots.reach(&root.join(path)), Reach::Outside(_)),
                "{path} was not reported as outside: {:?}",
                roots.reach(&root.join(path))
            );
        }

        // A cycle terminates, and is not an escape.
        assert!(matches!(
            roots.reach(&root.join("cycle_a")),
            Reach::Unresolvable(_)
        ));
    }

    // `canonicalize` gives up past `PATH_MAX` while the kernel keeps going,
    // resolving incrementally. The expanded path grows while the caller's stays
    // short, so `read_link` starts failing with `ENAMETOOLONG` on a path an
    // operation would still open. Reading that as "not a link" walks straight
    // out of the root.
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
                Err(Refusal::Unresolvable(_))
            ),
            "a path we cannot examine was not refused"
        );
    }

    // `confine` on top of that: what the caller gets back, and which of the two
    // things it does with a link at the end of the path.
    #[cfg(unix)]
    #[test]
    fn confine_returns_the_lexical_path_it_checked() {
        let scratch = ScratchDir::new("confine");
        let root = scratch.root();

        link("../outside", root.join("escape"));
        link("nowhere.txt", root.join("dangling"));

        // Symlinks are left in the returned path: the operation acts on what it
        // was asked for.
        assert_eq!(
            confine(&root, "dangling", FinalLink::Followed).unwrap(),
            root.join("dangling")
        );
        assert!(matches!(
            confine(&root, "escape/planted.txt", FinalLink::Followed),
            Err(Refusal::Outside(_))
        ));

        // Unlinking judges only what the link is reached through, so a link out
        // of the directory can be removed but not followed.
        assert_eq!(
            confine(&root, "escape", FinalLink::Unlinked).unwrap(),
            root.join("escape")
        );
        assert!(matches!(
            confine(&root, "escape", FinalLink::Followed),
            Err(Refusal::Outside(_))
        ));
        // Reaching *through* it is refused either way.
        assert!(matches!(
            confine(&root, "escape/secret.txt", FinalLink::Unlinked),
            Err(Refusal::Outside(_))
        ));

        // The root itself is in scope for both, and `..` at the root is not.
        assert_eq!(confine(&root, "/", FinalLink::Unlinked).unwrap(), root);
        assert_eq!(confine(&root, ".", FinalLink::Followed).unwrap(), root);
        assert!(matches!(
            confine(&root, "..", FinalLink::Unlinked),
            Err(Refusal::Outside(_))
        ));
    }
}

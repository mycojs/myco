//! Deciding whether a path stays inside the directories a grant names.
//!
//! One decision, shared by the two places that make it: the module graph, bound
//! to the project ([`super::resolve`]), and directory tokens, bound to the
//! directory they were issued for ([`super::ops::filesystem`]). Those were
//! written separately, and each leak found in one of them was live in the other
//! -- #51, #61 and #63 are the same defect reported against different callers.
//!
//! Symlinks are the whole difficulty. The rule here is that a link is judged by
//! **where it points**, not by whether it currently resolves: `canonicalize`
//! fails on a dangling link, on a cycle, and past `PATH_MAX` while the kernel
//! still opens the file, so a failure to resolve is not evidence of safety, and
//! it is not evidence of an escape either. Chains are followed lexically to a
//! hop limit, and a chain we cannot finish walking is refused as
//! [`Reach::Unresolvable`] rather than being reported as an escape.
//!
//! What this cannot see is anything that is not in the path: a hardlink is a
//! second name for a file and `symlink_metadata` reports it as an ordinary
//! file, and a bind mount is invisible for the same reason. Neither can be
//! created through a token, so both need someone who already has write access
//! to the directory. There is also a window between the decision here and the
//! syscall that acts on it. Closing those means holding a directory descriptor
//! and using `openat` with `RESOLVE_BENEATH`, which is a different design.

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
    /// A link that cannot be shown to stay inside: a cycle, a chain longer than
    /// [`MAX_HOPS`], one whose target cannot be placed because no parent of it
    /// resolves, or one that cannot be read at all. Refused like
    /// [`Reach::Outside`], but it is a different fact, and callers that can
    /// afford to say so should: reporting a dangling link inside the directory
    /// as "outside the token's scope" sends whoever reads it looking for an
    /// escape that is not there.
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

    /// Answered from the nearest ancestor that resolves, so a file, a directory
    /// and something absent all look alike from here.
    pub fn reach(&self, path: &Path) -> Reach {
        self.reach_within(path, MAX_HOPS)
    }

    /// `fuel` bounds how many times a link target is re-examined. Each level is
    /// a link whose target could not be resolved and had to be judged by the
    /// name it spells, and that name may itself run through a link.
    fn reach_within(&self, path: &Path, fuel: usize) -> Reach {
        let Some(fuel) = fuel.checked_sub(1) else {
            return Reach::Unresolvable(path.to_path_buf());
        };

        let mut probe = Some(path);
        while let Some(current) = probe {
            if !self.contains(current) {
                // The climb has walked above the roots. Lexical containment of
                // the path itself is [`Roots::contains`]'s job; an ancestor of
                // a root is not something the path was reached *through*.
                return Reach::Inside;
            }
            if let Ok(resolved) = current.canonicalize() {
                return if self.contains(&resolved) {
                    Reach::Inside
                } else {
                    Reach::Outside(resolved)
                };
            }
            if current.is_symlink() {
                // A link we cannot follow is not a link shown to stay inside.
                return match link_target(current) {
                    // Lexically inside is not the same as landing inside: the
                    // target names components of its own, and any of those may
                    // be a link that leads out. `link_target` walks the chain of
                    // links, not the path each one spells, so where the target
                    // lands is the same question again about a different path.
                    Some(target) if self.contains(&target) => self.reach_within(&target, fuel),
                    Some(target) => Reach::Outside(target),
                    None => Reach::Unresolvable(current.to_path_buf()),
                };
            }
            probe = current.parent();
        }
        Reach::Inside
    }
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

/// Where one hop lands. A relative target cannot be placed without a resolved
/// parent, and one that would not resolve is no licence to guess: the race that
/// produces it is exactly when guessing is wrong.
fn hop_target(target: PathBuf, resolved_parent: Option<PathBuf>) -> Option<PathBuf> {
    if target.is_absolute() {
        return Some(lexically_normalize(&target));
    }
    Some(lexically_normalize(&resolved_parent?.join(target)))
}

/// Follow a symlink chain without needing it to resolve, so a link is judged by
/// where it points.
fn link_target(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_HOPS {
        let target = std::fs::read_link(&current).ok()?;
        // A symlinked ancestor makes the lexical parent the wrong one to join
        // against, so the hop is placed against the resolved one.
        let parent = current
            .parent()
            .and_then(|parent| parent.canonicalize().ok());
        current = hop_target(target, parent)?;
        if !current.is_symlink() {
            return Some(current);
        }
    }
    None
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
    fn a_hop_cannot_be_placed_without_a_resolved_parent() {
        // An absolute target needs no parent.
        assert_eq!(
            hop_target(PathBuf::from("/a/b.ts"), None),
            Some(PathBuf::from("/a/b.ts"))
        );
        // A relative one does, and no parent means no answer rather than a guess.
        assert_eq!(hop_target(PathBuf::from("b.ts"), None), None);
        assert_eq!(
            hop_target(PathBuf::from("../b.ts"), Some(PathBuf::from("/p/sub"))),
            Some(PathBuf::from("/p/b.ts"))
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

//! Confining a path to the directory a grant names, by having the kernel do it.
//!
//! A [`Root`] holds an open descriptor for the granted directory, and every
//! path is resolved by `openat2` beneath that descriptor. Nothing is resolved
//! in userspace and no rebuilt path string is handed back to the OS, so there
//! is no second resolver to agree with.
//!
//! That matters because agreeing was the problem. Three earlier versions of
//! this module resolved paths themselves and handed the result to `std::fs`,
//! which resolved it again by the kernel's rules; each one differed from those
//! rules somewhere, and each difference was an escape -- a link target judged
//! by the path it spelled, a `..` collapsed before the link preceding it was
//! followed, a `read_link` failure past `PATH_MAX` read as "not a link". The
//! obligation was to reimplement `namei` exactly, which is not an obligation
//! worth accepting when the kernel will do it.
//!
//! `RESOLVE_BENEATH` refuses any resolution that leaves the descriptor:
//! absolute symlinks, absolute paths, and `..` above the root all fail with
//! `EXDEV`, while `..` that stays inside is applied to what was actually
//! resolved. A leading `/` is trimmed first, because for a directory token it
//! names the root of the grant rather than the root of the filesystem.
//!
//! What is left uncovered is smaller than before. The check and the operation
//! are now one syscall, so there is no window between them; a directory that is
//! renamed or replaced after the grant does not move the descriptor. A hardlink
//! is still a second name for a file and no path-based rule can see it, and it
//! still takes an exec token running `ln` -- or someone outside the runtime --
//! to create one.

use std::io;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Component, Path, PathBuf};

use rustix::fs::{Mode, OFlags, ResolveFlags};
use rustix::io::Errno;

/// Why an open under a root failed.
#[derive(Debug)]
pub enum OpenError {
    /// The path resolved out of the root. The one refusal the capability model
    /// cares about, and the only one worth a distinct message.
    Outside,
    /// Everything else, reported as the operation would report it: absent,
    /// not a directory, a symlink loop, permission denied.
    Io(io::Error),
}

impl OpenError {
    fn from_errno(errno: Errno) -> OpenError {
        match errno {
            // What `RESOLVE_BENEATH` refuses with.
            Errno::XDEV => OpenError::Outside,
            other => OpenError::Io(other.into()),
        }
    }
}

/// A granted directory, held open.
///
/// The descriptor is the grant. It cannot be repointed by renaming or replacing
/// the directory it was opened from, which is what made a stored path a grant
/// to whatever that name resolved to at the time of each call.
#[derive(Debug)]
pub struct Root {
    dir: OwnedFd,
    /// For error messages and for identity only -- never resolved again.
    display: PathBuf,
}

impl Root {
    /// Open `path` as a root. The path is resolved once, here, by the caller's
    /// own authority; from then on only the descriptor is used.
    pub fn open(path: &Path) -> io::Result<Root> {
        let dir = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let root = Root {
            dir,
            display: path.to_path_buf(),
        };

        // `openat2` is Linux 5.6 and later. Without it nothing here can confine
        // anything, and the failure would otherwise surface as every path under
        // the grant appearing not to exist. Say so once, at the grant, rather
        // than letting it look like an empty directory.
        if let Err(OpenError::Io(e)) =
            root.open_at(".", OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty())
        {
            if e.raw_os_error() == Some(Errno::NOSYS.raw_os_error()) {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "this kernel has no openat2 (Linux 5.6 or later is required), \
                     so a directory grant cannot be confined",
                ));
            }
            return Err(e);
        }

        Ok(root)
    }

    pub fn display(&self) -> &Path {
        &self.display
    }

    /// A second descriptor for the same directory, for handing to the thread
    /// that will do the work. It is the same grant: `dup` shares the open file
    /// description, so this cannot widen anything.
    pub fn try_clone(&self) -> io::Result<Root> {
        Ok(Root {
            dir: self.dir.try_clone()?,
            display: self.display.clone(),
        })
    }

    /// Like [`Root::open_at`] but with the flags taken exactly as given.
    ///
    /// Only `exec` needs this: it reaches the resolved file through
    /// `/proc/self/fd`, which the child must still be able to open, so the
    /// descriptor must not be close-on-exec.
    pub fn open_at_exact(
        &self,
        rel: &str,
        flags: OFlags,
        mode: Mode,
    ) -> Result<OwnedFd, OpenError> {
        let rel = relative(rel);
        rustix::fs::openat2(
            self.dir.as_fd(),
            rel.as_str(),
            flags,
            mode,
            ResolveFlags::BENEATH,
        )
        .map_err(OpenError::from_errno)
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.dir.as_fd()
    }

    /// Open `rel` beneath the root.
    ///
    /// Symlinks are followed, and are refused exactly when following one would
    /// leave the root. A link inside the root that points at something absent
    /// is an ordinary `ENOENT`, not an escape, which is what lets a write
    /// through one create the file it names.
    pub fn open_at(&self, rel: &str, flags: OFlags, mode: Mode) -> Result<OwnedFd, OpenError> {
        let rel = relative(rel);
        rustix::fs::openat2(
            self.dir.as_fd(),
            rel.as_str(),
            flags | OFlags::CLOEXEC,
            mode,
            ResolveFlags::BENEATH,
        )
        .map_err(OpenError::from_errno)
    }

    /// Resolve the directory containing `rel` and return it with the final
    /// component, for the operations that act on a name rather than on what it
    /// points at: `unlinkat`, `mkdirat`, `renameat`.
    ///
    /// This is what used to be `FinalLink::Unlinked`. It is no longer a choice
    /// the caller makes and gets wrong: `unlinkat` does not follow a final
    /// symlink, by construction, so removing a link that points out of the root
    /// removes the link and touches nothing at the far end.
    pub fn parent_at(&self, rel: &str) -> Result<(OwnedFd, String), OpenError> {
        let rel = relative(rel);
        let (parent, name) = match rel.rsplit_once('/') {
            Some((parent, name)) => (parent, name),
            None => ("", rel.as_str()),
        };

        // `.` and `..` name a directory, not an entry within one, so there is
        // no final component to act on.
        if name.is_empty() || name == "." || name == ".." {
            return Err(OpenError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path does not name an entry",
            )));
        }

        let parent = self.open_at(
            if parent.is_empty() { "." } else { parent },
            OFlags::RDONLY | OFlags::DIRECTORY,
            Mode::empty(),
        )?;
        Ok((parent, name.to_string()))
    }
}

/// A grant for one file: the directory it lives in, held open, and its name.
///
/// The name is deliberately not resolved when the grant is made -- a file token
/// may name something that does not exist yet, and one for a symlink should go
/// on meaning the symlink. What is pinned is the directory, which is what stops
/// the grant moving.
#[derive(Debug)]
pub struct FileGrant {
    pub dir: Root,
    pub name: String,
}

/// A leading `/` names the root of the grant, not the root of the filesystem,
/// and `RESOLVE_BENEATH` refuses an absolute path outright. Trimming it is a
/// pure string operation on what the caller passed, decided before anything
/// touches the filesystem, so it cannot be what a symlink subverts.
///
/// An empty path means the root itself.
fn relative(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        ".".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Collapse `.` and `..` without touching the filesystem.
///
/// Only for deciding what a *module specifier* names before anything is opened
/// -- see [`super::resolve`]. It is not a containment check and must not be
/// used as one: collapsing `..` without following what precedes it is exactly
/// the reasoning this module exists to avoid.
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
    use std::fs;
    use std::io::Read;

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
    fn a_leading_slash_names_the_root_of_the_grant() {
        assert_eq!(relative("/file.txt"), "file.txt");
        assert_eq!(relative("///file.txt"), "file.txt");
        assert_eq!(relative("file.txt"), "file.txt");
        assert_eq!(relative("/"), ".");
        assert_eq!(relative(""), ".");
    }

    struct ScratchDir {
        path: PathBuf,
    }

    impl ScratchDir {
        fn new(name: &str) -> ScratchDir {
            let path =
                std::env::temp_dir().join(format!("myco-confine-{}-{}", std::process::id(), name));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(path.join("root/sub/deep")).unwrap();
            fs::create_dir_all(path.join("outside")).unwrap();
            fs::write(path.join("outside/secret.txt"), "secret").unwrap();
            fs::write(path.join("root/file.txt"), "inside").unwrap();
            ScratchDir {
                path: path.canonicalize().unwrap(),
            }
        }

        fn root(&self) -> Root {
            Root::open(&self.path.join("root")).unwrap()
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn link(original: &str, at: PathBuf) {
        std::os::unix::fs::symlink(original, at).unwrap();
    }

    fn read(root: &Root, rel: &str) -> Result<String, OpenError> {
        let fd = root.open_at(rel, OFlags::RDONLY, Mode::empty())?;
        let mut text = String::new();
        std::fs::File::from(fd)
            .read_to_string(&mut text)
            .map_err(OpenError::Io)?;
        Ok(text)
    }

    fn is_outside(result: Result<String, OpenError>) -> bool {
        matches!(result, Err(OpenError::Outside))
    }

    fn is_absent(result: Result<String, OpenError>) -> bool {
        matches!(&result, Err(OpenError::Io(e)) if e.kind() == io::ErrorKind::NotFound)
    }

    // Every shape that reaches out, crossed with every state the far end can be
    // in. The three escapes this module was rewritten for are the `via_*` rows:
    // each was admitted by a userspace resolver that judged a link by the path
    // its target spelled.
    #[test]
    fn nothing_resolves_out_of_the_root() {
        let scratch = ScratchDir::new("out");
        let root = scratch.root();
        let outside = scratch.path.join("outside");

        link("../outside", scratch.path.join("root/escape"));
        link("../outside/deep", scratch.path.join("root/deep_escape"));
        fs::create_dir(outside.join("deep")).unwrap();
        link(
            outside.join("secret.txt").to_str().unwrap(),
            scratch.path.join("root/absolute"),
        );
        // Target spells a path inside the root but is reached through a link out.
        link("escape/secret.txt", scratch.path.join("root/via_escape"));
        link("via_escape", scratch.path.join("root/via_escape_twice"));
        // The escaping component erased by a `..`, which no lexical collapse
        // can see.
        link(
            "deep_escape/../secret.txt",
            scratch.path.join("root/via_dotdot"),
        );

        for rel in [
            "../outside/secret.txt",
            "escape",
            "escape/secret.txt",
            "escape/absent.txt",
            "absolute",
            "via_escape",
            "via_escape_twice",
            "via_dotdot",
            "deep_escape/../secret.txt",
            "/../outside/secret.txt",
            "sub/../../outside/secret.txt",
        ] {
            assert!(is_outside(read(&root, rel)), "{rel} was not refused");
        }
    }

    // Refusing too much is as much a defect as admitting too little, and it is
    // what #61 was about: a dangling or outward-pointing link inside the root
    // must not read as an escape.
    #[test]
    fn everything_inside_the_root_resolves() {
        let scratch = ScratchDir::new("in");
        let root = scratch.root();

        link("file.txt", scratch.path.join("root/live"));
        link("live", scratch.path.join("root/chain"));
        link("nowhere.txt", scratch.path.join("root/dangling"));
        link("sub/deep", scratch.path.join("root/deepin"));

        assert_eq!(read(&root, "file.txt").unwrap(), "inside");
        assert_eq!(read(&root, "/file.txt").unwrap(), "inside");
        assert_eq!(read(&root, "./file.txt").unwrap(), "inside");
        assert_eq!(read(&root, "sub/../file.txt").unwrap(), "inside");
        assert_eq!(read(&root, "live").unwrap(), "inside");
        assert_eq!(read(&root, "chain").unwrap(), "inside");

        // Absent, and inside: an ordinary miss, distinguishable from an escape.
        assert!(is_absent(read(&root, "absent.txt")));
        assert!(is_absent(read(&root, "dangling")));
        // `..` is applied to what the link resolved to, so this is `sub/x.txt`
        // -- which is absent -- rather than `x.txt` at the root (#71).
        assert!(is_absent(read(&root, "deepin/../x.txt")));
    }

    // Writing through a link that points inside the root creates what it names.
    #[test]
    fn a_write_through_a_dangling_link_creates_its_target() {
        let scratch = ScratchDir::new("write");
        let root = scratch.root();
        link("created.txt", scratch.path.join("root/pending"));

        let fd = root
            .open_at(
                "pending",
                OFlags::WRONLY | OFlags::CREATE,
                Mode::from_raw_mode(0o644),
            )
            .unwrap();
        use std::io::Write;
        std::fs::File::from(fd).write_all(b"through").unwrap();

        assert_eq!(
            fs::read_to_string(scratch.path.join("root/created.txt")).unwrap(),
            "through"
        );
    }

    // `parent_at` is what removal uses, and it must not follow the final link.
    #[test]
    fn a_name_is_resolved_without_following_its_final_link() {
        let scratch = ScratchDir::new("parent");
        let root = scratch.root();
        link("../outside/secret.txt", scratch.path.join("root/escaping"));

        // The link itself is inside the root, so its name resolves...
        let (parent, name) = root.parent_at("escaping").unwrap();
        assert_eq!(name, "escaping");
        rustix::fs::unlinkat(parent.as_fd(), name, rustix::fs::AtFlags::empty()).unwrap();
        // ...and unlinking it left what it pointed at alone.
        assert!(scratch.path.join("outside/secret.txt").exists());

        // What it is reached *through* is still confined.
        link("../outside", scratch.path.join("root/escape"));
        assert!(matches!(
            root.parent_at("escape/secret.txt"),
            Err(OpenError::Outside)
        ));
        assert!(matches!(
            root.parent_at("../outside/secret.txt"),
            Err(OpenError::Outside)
        ));
    }

    // `canonicalize` gives up past `PATH_MAX` while the kernel keeps resolving
    // incrementally. A userspace resolver reading that failure as "not a link"
    // walks straight out; the kernel has no such gap.
    #[test]
    fn a_path_too_long_to_canonicalize_is_still_confined() {
        const PATH_MAX: usize = 4096;
        let scratch = ScratchDir::new("toolong");
        let root_path = scratch.path.join("root");
        let root = scratch.root();

        let seg = "d".repeat(200);
        let last = "e".repeat(200);
        let depth = (PATH_MAX - 6 - root_path.to_string_lossy().len()) / (seg.len() + 1);
        let mut deep = root_path.clone();
        for _ in 0..depth {
            deep.push(&seg);
        }
        fs::create_dir_all(&deep).unwrap();
        link(
            deep.strip_prefix(&root_path).unwrap().to_str().unwrap(),
            root_path.join("a0"),
        );
        fs::create_dir(root_path.join("a0").join(&last)).unwrap();
        link(
            scratch.path.join("outside/secret.txt").to_str().unwrap(),
            root_path.join("a0").join(&last).join("escape"),
        );
        link(
            PathBuf::from("a0").join(&last).to_str().unwrap(),
            root_path.join("a"),
        );

        assert!(
            root_path.join("a/escape").canonicalize().is_err(),
            "fixture: canonicalize must fail"
        );
        assert!(is_outside(read(&root, "a/escape")));
    }

    // The descriptor is the grant: renaming or replacing the directory it was
    // opened from does not move it.
    #[test]
    fn a_grant_cannot_be_repointed_by_replacing_the_directory() {
        let scratch = ScratchDir::new("repoint");
        let root = scratch.root();

        fs::rename(scratch.path.join("root"), scratch.path.join("moved")).unwrap();
        link("../outside", scratch.path.join("root"));

        // Still the directory that was granted, not what the name now means.
        assert_eq!(read(&root, "file.txt").unwrap(), "inside");
        assert!(is_absent(read(&root, "secret.txt")));
    }
}

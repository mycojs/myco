//! Specifier to path, confined to the project. Separate from loading so a path has one spelling.

use std::path::{Path, PathBuf};

use log::debug;

use crate::errors::MycoError;
use crate::run::confine::{lexically_normalize, Reach, Roots};

/// Directories the module graph may load from. `import` is syntax, so the bound is structural.
#[derive(Debug, Clone, Default)]
pub struct ModuleRoots {
    roots: Roots,
}

impl ModuleRoots {
    /// Aliases resolve against the importer's dir, not `project_root`; they agree only at the root.
    pub fn new<I, P>(project_root: &Path, extra: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let project_root = canonical_or_normalized(project_root);
        let mut roots = vec![project_root.clone()];
        for path in extra {
            let path = path.as_ref();
            let joined = if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_root.join(path)
            };
            // Candidates are matched lexically, so a symlinked root needs both spellings.
            let lexical = lexically_normalize(&joined);
            let canonical = joined.canonicalize().unwrap_or_else(|_| lexical.clone());
            for resolved in [lexical, canonical] {
                // `..` or `/` would switch confinement off.
                if project_root.starts_with(&resolved) {
                    debug!(
                        "Ignoring myco-local resolve path '{}': it contains the project root",
                        resolved.display()
                    );
                    continue;
                }
                if !roots.contains(&resolved) {
                    roots.push(resolved);
                }
            }
        }
        ModuleRoots {
            roots: Roots::new(roots),
        }
    }

    fn contains(&self, path: &Path) -> bool {
        self.roots.contains(path)
    }
}

/// A checked module path. Only [`resolve`] builds one, so holding it proves it was in the roots
/// when it was checked -- the loader reopens it by path, so a concurrent swap is not covered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModule {
    path: PathBuf,
}

impl ResolvedModule {
    pub fn into_path(self) -> PathBuf {
        self.path
    }
}

/// Outside `roots` always fails as [`MycoError::ModuleOutsideProject`]; `ModuleNotFound` is inside.
pub fn resolve(
    specifier: &str,
    base_path: &Path,
    roots: &ModuleRoots,
) -> Result<ResolvedModule, MycoError> {
    let outside = || MycoError::ModuleOutsideProject {
        specifier: specifier.to_string(),
    };

    let path = specifier
        .strip_prefix("file://")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(specifier));

    let joined = if path.is_absolute() {
        path
    } else {
        base_path.join(path.strip_prefix("./").unwrap_or(&path))
    };
    let candidate = lexically_normalize(&joined);

    let path = match classify(roots, &candidate) {
        Classification::Outside => return Err(outside()),
        Classification::File => candidate,
        Classification::Directory => resolve_directory_index(roots, specifier, &candidate)?,
        Classification::Missing => return Err(not_found(specifier, &candidate)),
    };

    Ok(ResolvedModule { path })
}

/// What resolution is allowed to learn about a path, and the only thing that
/// looks at the filesystem to decide it.
enum Classification {
    Outside,
    Missing,
    File,
    Directory,
}

/// Check containment before metadata. Unresolvable paths also report `Outside`
/// so imports cannot distinguish the state of targets beyond the project.
fn classify(roots: &ModuleRoots, path: &Path) -> Classification {
    if !roots.contains(path) || roots.roots.reach(path) != Reach::Inside {
        return Classification::Outside;
    }
    // Following links is safe now: the escape check ruled out leaving the roots.
    match path.metadata() {
        Ok(metadata) if metadata.is_dir() => Classification::Directory,
        Ok(_) => Classification::File,
        Err(_) => Classification::Missing,
    }
}

/// Each candidate is classified before it is looked at, so an index file
/// pointing out of the project cannot answer with its target's existence.
fn resolve_directory_index(
    roots: &ModuleRoots,
    specifier: &str,
    dir: &Path,
) -> Result<PathBuf, MycoError> {
    for name in ["index.ts", "index.tsx", "index.js", "index.jsx"] {
        let candidate = dir.join(name);
        match classify(roots, &candidate) {
            Classification::Outside => {
                return Err(MycoError::ModuleOutsideProject {
                    specifier: specifier.to_string(),
                })
            }
            Classification::File => return Ok(candidate),
            Classification::Missing | Classification::Directory => continue,
        }
    }
    Err(MycoError::ModuleNotFound {
        specifier: specifier.to_string(),
        resolved_path: format!("{} (directory with no index file)", dir.display()),
    })
}

fn not_found(specifier: &str, path: &Path) -> MycoError {
    MycoError::ModuleNotFound {
        specifier: specifier.to_string(),
        resolved_path: path.display().to_string(),
    }
}

/// A root may not exist: `resolve` lists are tried in order, so some are expected to miss.
fn canonical_or_normalized(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| lexically_normalize(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Non-existent paths, so these exercise the lexical path.
    fn roots(project_root: &str, extra: &[&str]) -> ModuleRoots {
        ModuleRoots::new(Path::new(project_root), extra)
    }

    fn contains(roots: &ModuleRoots, path: &str) -> bool {
        roots.contains(&lexically_normalize(Path::new(path)))
    }

    #[test]
    fn project_root_bounds_the_module_graph() {
        let roots = roots("/proj", &[]);

        assert!(contains(&roots, "/proj/src/index.ts"));
        assert!(contains(&roots, "/proj/vendor/@myco/check/index.ts"));
        assert!(contains(&roots, "/proj"));

        assert!(!contains(&roots, "/proj/../outside/evil.ts"));
        assert!(!contains(&roots, "/etc/passwd"));
        // A name that merely starts with the root's is not inside it.
        assert!(!contains(&roots, "/project-other/evil.ts"));
    }

    #[test]
    fn myco_local_paths_extend_the_roots() {
        // What `myco ws install` writes.
        let roots = roots("/workspace/cli", &["../lib/std"]);

        assert!(contains(&roots, "/workspace/cli/src/main.ts"));
        assert!(contains(&roots, "/workspace/lib/std/src/index.ts"));
        assert!(!contains(&roots, "/workspace/lib/other/index.ts"));
        assert!(!contains(&roots, "/workspace/secrets.ts"));
    }

    #[test]
    fn absolute_myco_local_paths_are_taken_as_given() {
        let roots = roots("/proj", &["/elsewhere/lib"]);

        assert!(contains(&roots, "/elsewhere/lib/index.ts"));
        assert!(!contains(&roots, "/elsewhere/other/index.ts"));
    }

    #[test]
    fn refuses_myco_local_roots_that_swallow_the_project() {
        let swallowing = roots("/workspace/cli", &["..", "../..", "/"]);

        assert!(contains(&swallowing, "/workspace/cli/src/main.ts"));
        assert!(!contains(&swallowing, "/workspace/other/secrets.ts"));
        assert!(!contains(&swallowing, "/etc/passwd"));

        // A sibling is not an ancestor.
        let sibling = roots("/workspace/cli", &["../lib/std"]);
        assert!(contains(&sibling, "/workspace/lib/std/index.ts"));
    }

    // Everything below needs a real filesystem: symlinks are the whole point.
    struct ScratchDir {
        path: PathBuf,
    }

    impl ScratchDir {
        fn new(name: &str) -> ScratchDir {
            let path =
                std::env::temp_dir().join(format!("myco-resolve-{}-{}", std::process::id(), name));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(path.join("proj")).unwrap();
            fs::create_dir_all(path.join("outside")).unwrap();
            fs::write(path.join("outside/secret.ts"), "export const x = 1;").unwrap();
            ScratchDir {
                path: path.canonicalize().unwrap(),
            }
        }

        fn proj(&self) -> PathBuf {
            self.path.join("proj")
        }

        fn roots(&self) -> ModuleRoots {
            ModuleRoots::new(&self.proj(), Vec::<&Path>::new())
        }

        fn resolve(&self, specifier: &str) -> Result<ResolvedModule, MycoError> {
            resolve(specifier, &self.proj(), &self.roots())
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn is_outside(result: &Result<ResolvedModule, MycoError>) -> bool {
        matches!(result, Err(MycoError::ModuleOutsideProject { .. }))
    }

    #[test]
    fn resolves_modules_inside_the_project() {
        let scratch = ScratchDir::new("inside");
        fs::write(scratch.proj().join("main.ts"), "export const x = 1;").unwrap();
        fs::create_dir(scratch.proj().join("sub")).unwrap();
        fs::write(scratch.proj().join("sub/index.ts"), "export const y = 2;").unwrap();

        assert_eq!(
            scratch.resolve("./main.ts").unwrap().into_path(),
            scratch.proj().join("main.ts")
        );
        // `..` that lands back inside is applied, not refused.
        assert_eq!(
            scratch.resolve("./sub/../main.ts").unwrap().into_path(),
            scratch.proj().join("main.ts")
        );
        assert_eq!(
            scratch.resolve("./sub").unwrap().into_path(),
            scratch.proj().join("sub/index.ts")
        );
        // Absent, but inside: the caller is allowed to know that.
        assert!(matches!(
            scratch.resolve("./nope.ts"),
            Err(MycoError::ModuleNotFound { .. })
        ));
    }

    #[test]
    fn refuses_every_way_out_of_the_project() {
        let scratch = ScratchDir::new("outside");

        assert!(is_outside(&scratch.resolve("../outside/secret.ts")));
        assert!(is_outside(&scratch.resolve("/etc/passwd")));
        assert!(is_outside(&scratch.resolve(&format!(
            "file://{}",
            scratch.path.join("outside/secret.ts").display()
        ))));
        assert!(is_outside(&scratch.resolve("./../outside")));
    }

    #[test]
    fn refusal_does_not_reveal_whether_the_target_exists() {
        let scratch = ScratchDir::new("oracle");
        let outside = scratch.path.join("outside");
        std::os::unix::fs::symlink(outside.join("secret.ts"), scratch.proj().join("live.ts"))
            .unwrap();
        std::os::unix::fs::symlink(outside.join("missing.ts"), scratch.proj().join("dead.ts"))
            .unwrap();
        std::os::unix::fs::symlink(&outside, scratch.proj().join("dir")).unwrap();

        let probes = [
            "./live.ts",       // symlink to a file that exists
            "./dead.ts",       // symlink to one that does not
            "./dir",           // symlink to a directory
            "./dir/secret.ts", // through a symlinked directory
            "./dir/missing.ts",
            "../outside/secret.ts", // no symlink at all
            "../outside/missing.ts",
        ];
        for probe in probes {
            let result = scratch.resolve(probe);
            assert!(is_outside(&result), "{probe} leaked: {result:?}");
        }
    }

    #[test]
    fn empty_roots_load_nothing() {
        // What an isolate with no state gets.
        assert!(!ModuleRoots::default().contains(Path::new("/anything")));
    }

    #[test]
    fn refuses_a_symlinked_index_file() {
        let scratch = ScratchDir::new("index");
        fs::create_dir(scratch.proj().join("dir")).unwrap();
        std::os::unix::fs::symlink(
            scratch.path.join("outside/secret.ts"),
            scratch.proj().join("dir/index.ts"),
        )
        .unwrap();

        assert!(is_outside(&scratch.resolve("./dir")));
    }

    #[test]
    fn admits_a_root_named_through_a_symlink() {
        let scratch = ScratchDir::new("symroot");
        fs::create_dir(scratch.path.join("reallib")).unwrap();
        fs::write(
            scratch.path.join("reallib/index.ts"),
            "export const lib = 1;",
        )
        .unwrap();
        std::os::unix::fs::symlink("reallib", scratch.path.join("liblink")).unwrap();

        let roots = ModuleRoots::new(&scratch.proj(), ["../liblink"]);
        assert!(resolve("../liblink/index.ts", &scratch.proj(), &roots).is_ok());
        // The root is what was named, not everything beside it.
        assert!(is_outside(&resolve(
            "../outside/secret.ts",
            &scratch.proj(),
            &roots
        )));
    }

    // Only the live index is caught unless presence ignores the link.
    #[test]
    fn a_symlinked_index_hides_nothing_about_its_target() {
        let scratch = ScratchDir::new("index-oracle");
        let outside = scratch.path.join("outside");
        for (dir, target) in [("live", "secret.ts"), ("dead", "missing.ts")] {
            fs::create_dir(scratch.proj().join(dir)).unwrap();
            std::os::unix::fs::symlink(
                outside.join(target),
                scratch.proj().join(dir).join("index.ts"),
            )
            .unwrap();
        }

        let live = scratch.resolve("./live");
        let dead = scratch.resolve("./dead");
        assert!(is_outside(&live), "{live:?}");
        assert!(is_outside(&dead), "{dead:?}");
    }

    #[test]
    fn a_chain_too_long_to_follow_is_refused() {
        let scratch = ScratchDir::new("hops");
        let hops = 41;
        for i in 0..hops {
            let target = if i < hops - 1 {
                PathBuf::from(format!("link{}.ts", i + 1))
            } else {
                PathBuf::from("missing.ts")
            };
            std::os::unix::fs::symlink(target, scratch.proj().join(format!("link{i}.ts"))).unwrap();
        }

        let result = scratch.resolve("./link0.ts");
        assert!(is_outside(&result), "{result:?}");
    }

    // Every shape that should resolve. Refusing these is as much a defect as
    // admitting the ones below, and is what over-tightening breaks first.
    #[test]
    fn nothing_inside_the_project_is_refused() {
        let scratch = ScratchDir::new("inside-matrix");
        let proj = scratch.proj();
        fs::write(proj.join("leaf.ts"), "export const a = 1;").unwrap();
        fs::create_dir_all(proj.join("sub/deep")).unwrap();
        fs::write(proj.join("sub/index.ts"), "export const b = 2;").unwrap();
        fs::write(proj.join("sub/deep/index.js"), "export const c = 3;").unwrap();
        std::os::unix::fs::symlink("leaf.ts", proj.join("to_file.ts")).unwrap();
        std::os::unix::fs::symlink("sub", proj.join("to_dir")).unwrap();
        std::os::unix::fs::symlink("to_file.ts", proj.join("chain.ts")).unwrap();

        let absolute = proj.join("leaf.ts").display().to_string();
        let specifiers = vec![
            "./leaf.ts".to_string(),
            "leaf.ts".to_string(),
            "./sub/../leaf.ts".to_string(),
            "./sub/./index.ts".to_string(),
            "./sub".to_string(),
            "./sub/deep".to_string(),
            "./to_file.ts".to_string(),
            "./to_dir".to_string(),
            "./chain.ts".to_string(),
            absolute.clone(),
            format!("file://{absolute}"),
        ];
        for specifier in specifiers {
            let result = scratch.resolve(&specifier);
            assert!(result.is_ok(), "{specifier} was refused: {result:?}");
        }
    }

    // Absence inside the project is an ordinary error, not an escape.
    #[test]
    fn absence_inside_the_project_is_not_an_escape() {
        let scratch = ScratchDir::new("inside-missing");
        let proj = scratch.proj();
        fs::create_dir(proj.join("empty")).unwrap();
        std::os::unix::fs::symlink("nowhere.ts", proj.join("dangling.ts")).unwrap();

        for specifier in ["./nope.ts", "./empty", "./dangling.ts", "./a/b/c.ts"] {
            let result = scratch.resolve(specifier);
            assert!(
                matches!(result, Err(MycoError::ModuleNotFound { .. })),
                "{specifier}: {result:?}"
            );
        }
    }

    #[test]
    fn a_root_that_does_not_exist_yields_absence_not_escape() {
        let scratch = ScratchDir::new("ghost-root");
        let roots = ModuleRoots::new(&scratch.proj(), ["../ghost"]);

        let result = resolve("../ghost/x.ts", &scratch.proj(), &roots);
        assert!(
            matches!(result, Err(MycoError::ModuleNotFound { .. })),
            "{result:?}"
        );
    }

    // A dangling chain inside the project is absence, not an escape, for any
    // length the follower is willing to walk.
    #[test]
    fn a_dangling_chain_inside_the_project_is_absence() {
        let scratch = ScratchDir::new("inside-chain");
        for i in 0..8 {
            std::os::unix::fs::symlink(
                format!("hop{}.ts", i + 1),
                scratch.proj().join(format!("hop{i}.ts")),
            )
            .unwrap();
        }

        let result = scratch.resolve("./hop0.ts");
        assert!(
            matches!(result, Err(MycoError::ModuleNotFound { .. })),
            "{result:?}"
        );
    }

    // The escaping link need not be the leaf. A symlinked directory in the path
    // means the lexical parent is not the real one, so a target joined to it can
    // read as inside when it is not.
    #[test]
    fn a_link_under_a_symlinked_directory_is_judged_by_its_real_parent() {
        let scratch = ScratchDir::new("under-dir");
        let outside = scratch.path.join("outside");
        std::os::unix::fs::symlink("real.ts", outside.join("live")).unwrap();
        std::os::unix::fs::symlink("missing.ts", outside.join("dead")).unwrap();
        fs::write(outside.join("real.ts"), "export const x = 1;").unwrap();
        std::os::unix::fs::symlink(&outside, scratch.proj().join("d")).unwrap();

        for specifier in ["./d/live", "./d/dead", "./d/real.ts"] {
            let result = scratch.resolve(specifier);
            assert!(is_outside(&result), "{specifier}: {result:?}");
        }
    }

    // The only escaping link sits beyond PATH_MAX; earlier components stay inside.
    #[test]
    fn refuses_a_link_too_long_to_examine() {
        const PATH_MAX: usize = 4096;
        let scratch = ScratchDir::new("toolong");
        let proj = scratch.proj();
        let outside = scratch.path.join("outside");

        // Depth in two steps, so no path handed to the OS is itself over the
        // limit: a real tree that fits, then one more level through an alias.
        let seg = "d".repeat(200);
        let depth = (PATH_MAX - 6 - proj.to_string_lossy().len()) / (seg.len() + 1);
        let mut rel = PathBuf::new();
        for _ in 0..depth {
            rel.push(&seg);
        }
        fs::create_dir_all(proj.join(&rel)).unwrap();
        std::os::unix::fs::symlink(&rel, proj.join("a0")).unwrap();

        let last = "e".repeat(200);
        fs::create_dir(proj.join("a0").join(&last)).unwrap();
        std::os::unix::fs::symlink(
            outside.join("secret.ts"),
            proj.join("a0").join(&last).join("escape.ts"),
        )
        .unwrap();
        std::os::unix::fs::symlink(PathBuf::from("a0").join(&last), proj.join("a")).unwrap();

        let candidate = proj.join("a/escape.ts");
        assert!(
            candidate.canonicalize().is_err(),
            "fixture: canonicalize must fail"
        );
        assert!(
            fs::read_to_string(&candidate).is_ok(),
            "fixture: the OS must still reach outside through it"
        );

        let result = scratch.resolve("./a/escape.ts");
        assert!(is_outside(&result), "{result:?}");
    }

    // Every shape that can reach outside, crossed with every state the far end
    // can be in. Each leak so far has been one uncovered cell of this table.
    #[test]
    fn nothing_outside_the_project_is_distinguishable() {
        let scratch = ScratchDir::new("matrix");
        let outside = scratch.path.join("outside");
        fs::create_dir_all(outside.join("dir")).unwrap();
        let proj = scratch.proj();
        let mut specifiers: Vec<String> = Vec::new();

        for target in ["secret.ts", "missing.ts", "dir"] {
            specifiers.push(format!("../outside/{target}"));
        }

        for (name, target) in [
            ("leaf_live.ts", "secret.ts"),
            ("leaf_dead.ts", "missing.ts"),
            ("leaf_dir", "dir"),
        ] {
            std::os::unix::fs::symlink(outside.join(target), proj.join(name)).unwrap();
            specifiers.push(format!("./{name}"));
        }

        for (dir, target) in [("idx_live", "secret.ts"), ("idx_dead", "missing.ts")] {
            fs::create_dir(proj.join(dir)).unwrap();
            std::os::unix::fs::symlink(outside.join(target), proj.join(dir).join("index.ts"))
                .unwrap();
            specifiers.push(format!("./{dir}"));
        }

        for (prefix, hops, target) in [
            ("short_live", 2, "secret.ts"),
            ("short_dead", 2, "missing.ts"),
            ("long_live", 40, "secret.ts"),
            ("long_dead", 40, "missing.ts"),
        ] {
            for i in 0..hops {
                let link = if i < hops - 1 {
                    PathBuf::from(format!("{prefix}{}.ts", i + 1))
                } else {
                    outside.join(target)
                };
                std::os::unix::fs::symlink(link, proj.join(format!("{prefix}{i}.ts"))).unwrap();
            }
            specifiers.push(format!("./{prefix}0.ts"));
        }

        for specifier in &specifiers {
            let result = scratch.resolve(specifier);
            assert!(is_outside(&result), "{specifier} leaked: {result:?}");
        }
    }

    #[test]
    fn follows_symlink_chains_and_survives_cycles() {
        let scratch = ScratchDir::new("chain");
        // a -> b -> outside: the escape is only visible at the end of the chain.
        std::os::unix::fs::symlink("b.ts", scratch.proj().join("a.ts")).unwrap();
        std::os::unix::fs::symlink(
            scratch.path.join("outside/missing.ts"),
            scratch.proj().join("b.ts"),
        )
        .unwrap();
        assert!(is_outside(&scratch.resolve("./a.ts")));

        // A cycle must terminate rather than spin.
        std::os::unix::fs::symlink("two.ts", scratch.proj().join("one.ts")).unwrap();
        std::os::unix::fs::symlink("one.ts", scratch.proj().join("two.ts")).unwrap();
        assert!(scratch.resolve("./one.ts").is_err());
    }
}

//! Symbolic link checks for app-owned trees that root reads, copies or restores.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Links the kernel follows while resolving one path before giving up (`MAXSYMLINKS`).
const MAX_LINK_HOPS: usize = 40;

enum Part {
    Parent,
    Name(OsString),
}

/// Resolves the symbolic link at `link` the way the kernel would, following every link met on the
/// way, and reports whether the result stays inside `root`.
///
/// Checking the link text alone is not enough: `a/b/up -> ../..` and `esc -> a/b/up/../../..` each
/// look harmless, but together `esc` points two levels above `root`. Absolute targets are refused,
/// and components that do not exist yet are treated as plain directories.
pub(crate) fn link_resolves_inside(root: &Path, link: &Path) -> io::Result<bool> {
    let Some(parent) = link.parent() else {
        return Ok(false);
    };
    let Ok(relative_parent) = parent.strip_prefix(root) else {
        return Ok(false);
    };
    let mut current = Vec::new();
    for component in relative_parent.components() {
        match component {
            Component::Normal(name) => current.push(name.to_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return Ok(false),
        }
    }
    let mut pending = VecDeque::new();
    if !push_target(&mut pending, &fs::read_link(link)?) {
        return Ok(false);
    }
    let mut hops = 1_usize;
    while let Some(part) = pending.pop_front() {
        match part {
            Part::Parent => {
                if current.pop().is_none() {
                    return Ok(false);
                }
            }
            Part::Name(name) => {
                let candidate = current
                    .iter()
                    .fold(root.to_path_buf(), |path, segment| path.join(segment))
                    .join(&name);
                match fs::symlink_metadata(&candidate) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        hops += 1;
                        if hops > MAX_LINK_HOPS
                            || !push_target(&mut pending, &fs::read_link(&candidate)?)
                        {
                            return Ok(false);
                        }
                    }
                    Ok(_) => current.push(name),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => current.push(name),
                    Err(error) => return Err(error),
                }
            }
        }
    }
    Ok(true)
}

/// Queues the components of a relative link target ahead of whatever is still pending.
fn push_target(pending: &mut VecDeque<Part>, target: &Path) -> bool {
    let mut parts = Vec::new();
    for component in target.components() {
        match component {
            Component::Normal(name) => parts.push(Part::Name(name.to_owned())),
            Component::ParentDir => parts.push(Part::Parent),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    for part in parts.into_iter().rev() {
        pending.push_front(part);
    }
    true
}

/// Visits `root` and everything below it without following symbolic links: a link is visited as
/// itself and never descended into, so a link planted in app data cannot redirect the visit to a
/// path outside the tree.
// Only the Android SELinux relabel needs it outside of tests.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
pub(crate) fn visit_tree_nofollow(
    root: &Path,
    mut visit: impl FnMut(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut pending: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        visit(&path)?;
        if metadata.file_type().is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn chained_links_that_climb_out_of_the_root_are_detected() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("root");
        fs::create_dir_all(root.join("a/b"))?;
        symlink("../..", root.join("a/b/up"))?;
        symlink("a/b/up/../..", root.join("escape"))?;

        assert!(link_resolves_inside(&root, &root.join("a/b/up"))?);
        assert!(!link_resolves_inside(&root, &root.join("escape"))?);
        Ok(())
    }

    #[test]
    fn plain_relative_links_inside_the_root_are_accepted() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        fs::create_dir_all(root.join("files/nested"))?;
        symlink("../nested", root.join("files/nested/self"))?;
        symlink("files/missing", root.join("dangling"))?;
        symlink("..", root.join("files/parent"))?;

        assert!(link_resolves_inside(root, &root.join("files/nested/self"))?);
        assert!(link_resolves_inside(root, &root.join("dangling"))?);
        assert!(link_resolves_inside(root, &root.join("files/parent"))?);
        Ok(())
    }

    #[test]
    fn absolute_and_looping_links_are_refused() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        symlink("/data/system", root.join("absolute"))?;
        symlink("loop-b", root.join("loop-a"))?;
        symlink("loop-a", root.join("loop-b"))?;

        assert!(!link_resolves_inside(root, &root.join("absolute"))?);
        assert!(!link_resolves_inside(root, &root.join("loop-a"))?);
        Ok(())
    }

    #[test]
    fn tree_visit_never_follows_links_out_of_the_tree() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let outside = temp.path().join("outside");
        let root = temp.path().join("root");
        fs::create_dir_all(outside.join("secret"))?;
        fs::create_dir_all(root.join("dir"))?;
        fs::write(root.join("dir/file"), b"data")?;
        symlink(&outside, root.join("dir/link"))?;

        let mut visited = Vec::new();
        visit_tree_nofollow(&root, |path| {
            visited.push(path.to_path_buf());
            Ok(())
        })?;

        visited.sort();
        assert_eq!(
            visited,
            vec![
                root.clone(),
                root.join("dir"),
                root.join("dir/file"),
                root.join("dir/link"),
            ],
        );
        Ok(())
    }
}

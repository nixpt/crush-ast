//! Where the `--fs` sandbox's files live: the host's disk ([`DiskFs`]) or a
//! per-registry in-memory tree ([`MemFs`], for hosts with no disk to give,
//! such as a browser).
//!
//! Every `fs.*` / `text.*` file cap, and [`FsSandbox`](crate::host_caps::FsSandbox)'s
//! path resolution, goes through an [`FsBackend`], so there is one
//! implementation of each cap and of the sandbox rules (`..`, absolute paths,
//! symlinks, the working directory) whichever backend is underneath. A backend
//! only provides the storage primitives below. [`DiskFs`] is exactly the
//! `std::fs` calls the caps made before backends existed, including
//! [`open_for_write`](crate::host_caps::open_for_write)'s guards. [`MemFs`]
//! answers the same calls with the same results and the same errors, worded
//! as Linux words them (`No such file or directory (os error 2)`), so a
//! program prints the same thing on either. It has no symlinks.

use std::ffi::OsString;
use std::io;
use std::path::{Component, Path, PathBuf};

/// What a path names, as `symlink_metadata` / `metadata` report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    /// Anything else (a socket, a device, ...): not a regular file.
    Other,
}

/// One directory entry: its name, and its kind without following a symlink.
#[derive(Debug)]
pub struct Entry {
    pub name: OsString,
    pub kind: io::Result<Kind>,
}

/// The storage primitives the file caps use. Paths are absolute: the
/// sandbox has already resolved them under its root.
pub trait FsBackend: Send + Sync {
    /// `Path::canonicalize`.
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    /// What `path` itself is (`symlink_metadata`).
    fn symlink_kind(&self, path: &Path) -> io::Result<Kind>;
    /// What `path` names, following symlinks (`metadata`).
    fn kind(&self, path: &Path) -> io::Result<Kind>;
    fn read_link(&self, path: &Path) -> io::Result<PathBuf>;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    /// Create or truncate `path` and write `data` (`open_for_write(path, true)`).
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()>;
    /// Create `path` if it is missing, keep it if not, and mark it modified.
    fn touch(&self, path: &Path) -> io::Result<()>;
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// The entries of a directory, in no particular order.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<io::Result<Entry>>>;
    /// Copy a file's contents (and, on disk, its permissions) to `to`,
    /// opened as [`FsBackend::write`] opens it.
    fn copy_file(&self, from: &Path, to: &Path) -> io::Result<()>;
}

// ── the host's disk ──────────────────────────────────────────────────────────

/// The host's filesystem, through `std::fs`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DiskFs;

fn kind_of(t: std::fs::FileType) -> Kind {
    if t.is_symlink() {
        Kind::Symlink
    } else if t.is_dir() {
        Kind::Dir
    } else if t.is_file() {
        Kind::File
    } else {
        Kind::Other
    }
}

impl FsBackend for DiskFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        path.canonicalize()
    }
    fn symlink_kind(&self, path: &Path) -> io::Result<Kind> {
        std::fs::symlink_metadata(path).map(|m| kind_of(m.file_type()))
    }
    fn kind(&self, path: &Path) -> io::Result<Kind> {
        std::fs::metadata(path).map(|m| kind_of(m.file_type()))
    }
    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::read_link(path)
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        crate::host_caps::open_for_write(path, true).and_then(|mut file| io::Write::write_all(&mut file, data))
    }
    fn touch(&self, path: &Path) -> io::Result<()> {
        crate::host_caps::open_for_write(path, false)?.set_modified(std::time::SystemTime::now())
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<io::Result<Entry>>> {
        Ok(std::fs::read_dir(path)?
            .map(|e| e.map(|e| Entry { name: e.file_name(), kind: e.file_type().map(kind_of) }))
            .collect())
    }
    /// Contents and permissions, as `std::fs::copy` copies them, but with the
    /// destination opened by `open_for_write`, so it is never a symlink.
    fn copy_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut src = std::fs::File::open(from)?;
        let permissions = src.metadata()?.permissions();
        let mut dst = crate::host_caps::open_for_write(to, true)?;
        io::copy(&mut src, &mut dst)?;
        dst.set_permissions(permissions)
    }
}

// ── in memory ────────────────────────────────────────────────────────────────

/// Errors as Linux reports them, so a message reads the same as on disk.
fn os_error(kind: io::ErrorKind, code: i32) -> io::Error {
    let text = match code {
        2 => "No such file or directory",
        17 => "File exists",
        20 => "Not a directory",
        21 => "Is a directory",
        22 => "Invalid argument",
        39 => "Directory not empty",
        _ => "Unknown error",
    };
    io::Error::new(kind, format!("{text} (os error {code})"))
}
fn enoent() -> io::Error {
    os_error(io::ErrorKind::NotFound, 2)
}
fn eexist() -> io::Error {
    os_error(io::ErrorKind::AlreadyExists, 17)
}
fn enotdir() -> io::Error {
    os_error(io::ErrorKind::NotADirectory, 20)
}
fn eisdir() -> io::Error {
    os_error(io::ErrorKind::IsADirectory, 21)
}
fn einval() -> io::Error {
    os_error(io::ErrorKind::InvalidInput, 22)
}
fn enotempty() -> io::Error {
    os_error(io::ErrorKind::DirectoryNotEmpty, 39)
}

#[derive(Debug, Clone)]
enum Node {
    File(Vec<u8>),
    Dir(std::collections::BTreeMap<OsString, Node>),
}

/// A directory tree in memory, rooted at `/`. Empty when made; it lives as
/// long as the capability registry that holds it (one per run in crush-web).
#[derive(Debug)]
pub struct MemFs {
    root: std::sync::Mutex<Node>,
}

impl Default for MemFs {
    fn default() -> Self {
        Self::new()
    }
}

/// The names along an absolute path (`.` dropped, `..` applied: there are no
/// symlinks, so that is where the kernel would end up).
fn names(path: &Path) -> io::Result<Vec<OsString>> {
    let mut out: Vec<OsString> = Vec::new();
    for c in path.components() {
        match c {
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(n) => out.push(n.to_os_string()),
        }
    }
    // has_root, not is_absolute: on wasm32-unknown-unknown (crush-web) std
    // calls `/x` relative, since that target is neither unix nor windows.
    if !path.has_root() {
        return Err(enoent());
    }
    Ok(out)
}

impl MemFs {
    pub fn new() -> Self {
        Self { root: std::sync::Mutex::new(Node::Dir(Default::default())) }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Node) -> io::Result<T>) -> io::Result<T> {
        let mut root = self.root.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut root)
    }
}

/// The node at `names`: ENOENT for a missing one, ENOTDIR for a path that
/// goes through a file.
fn lookup<'a>(mut node: &'a Node, names: &[OsString]) -> io::Result<&'a Node> {
    for n in names {
        node = match node {
            Node::Dir(children) => children.get(n).ok_or_else(enoent)?,
            Node::File(_) => return Err(enotdir()),
        };
    }
    Ok(node)
}

fn lookup_mut<'a>(mut node: &'a mut Node, names: &[OsString]) -> io::Result<&'a mut Node> {
    for n in names {
        node = match node {
            Node::Dir(children) => children.get_mut(n).ok_or_else(enoent)?,
            Node::File(_) => return Err(enotdir()),
        };
    }
    Ok(node)
}

/// The directory that holds the last name, and that name. The root itself
/// has no parent: it exists and is a directory.
fn parent_mut<'a>(root: &'a mut Node, names: &'a [OsString]) -> io::Result<Option<(&'a mut std::collections::BTreeMap<OsString, Node>, &'a OsString)>> {
    let Some((last, dirs)) = names.split_last() else {
        return Ok(None);
    };
    match lookup_mut(root, dirs)? {
        Node::Dir(children) => Ok(Some((children, last))),
        Node::File(_) => Err(enotdir()),
    }
}

fn kind_of_node(node: &Node) -> Kind {
    match node {
        Node::File(_) => Kind::File,
        Node::Dir(_) => Kind::Dir,
    }
}

impl FsBackend for MemFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let names = names(path)?;
        self.with(|root| lookup(root, &names).map(|_| ()))?;
        let mut out = PathBuf::from("/");
        out.extend(&names);
        Ok(out)
    }
    fn symlink_kind(&self, path: &Path) -> io::Result<Kind> {
        self.kind(path)
    }
    fn kind(&self, path: &Path) -> io::Result<Kind> {
        let names = names(path)?;
        self.with(|root| lookup(root, &names).map(kind_of_node))
    }
    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        // Nothing here is a link: as readlink(2) on a file or directory.
        self.kind(path)?;
        Err(einval())
    }
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let names = names(path)?;
        let bytes = self.with(|root| match lookup(root, &names)? {
            Node::File(b) => Ok(b.clone()),
            Node::Dir(_) => Err(eisdir()),
        })?;
        String::from_utf8(bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "stream did not contain valid UTF-8"))
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        let names = names(path)?;
        self.with(|root| match parent_mut(root, &names)? {
            None => Err(eisdir()),
            Some((dir, name)) => match dir.get_mut(name) {
                Some(Node::Dir(_)) => Err(eisdir()),
                Some(Node::File(bytes)) => {
                    *bytes = data.to_vec();
                    Ok(())
                }
                None => {
                    dir.insert(name.clone(), Node::File(data.to_vec()));
                    Ok(())
                }
            },
        })
    }
    fn touch(&self, path: &Path) -> io::Result<()> {
        let names = names(path)?;
        self.with(|root| match parent_mut(root, &names)? {
            None => Err(eisdir()),
            Some((dir, name)) => match dir.get(name) {
                Some(Node::Dir(_)) => Err(eisdir()),
                Some(Node::File(_)) => Ok(()),
                None => {
                    dir.insert(name.clone(), Node::File(Vec::new()));
                    Ok(())
                }
            },
        })
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        let names = names(path)?;
        self.with(|root| match parent_mut(root, &names)? {
            None => Err(eexist()),
            Some((dir, name)) => {
                if dir.contains_key(name) {
                    return Err(eexist());
                }
                dir.insert(name.clone(), Node::Dir(Default::default()));
                Ok(())
            }
        })
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        let names = names(path)?;
        self.with(|root| {
            let mut node = root;
            for (i, n) in names.iter().enumerate() {
                let Node::Dir(children) = node else { unreachable!("only directories are descended into") };
                let child = children.entry(n.clone()).or_insert_with(|| Node::Dir(Default::default()));
                if let Node::File(_) = child {
                    // as std: the last name existing as a file is EEXIST;
                    // creating something under a file is ENOTDIR
                    return Err(if i + 1 == names.len() { eexist() } else { enotdir() });
                }
                node = child;
            }
            Ok(())
        })
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let names = names(path)?;
        self.with(|root| match parent_mut(root, &names)? {
            None => Err(eisdir()),
            Some((dir, name)) => match dir.get(name) {
                None => Err(enoent()),
                Some(Node::Dir(_)) => Err(eisdir()),
                Some(Node::File(_)) => {
                    dir.remove(name);
                    Ok(())
                }
            },
        })
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let names = names(path)?;
        self.with(|root| match parent_mut(root, &names)? {
            None => Err(einval()),
            Some((dir, name)) => match dir.get(name) {
                None => Err(enoent()),
                Some(Node::File(_)) => Err(enotdir()),
                Some(Node::Dir(_)) => {
                    dir.remove(name);
                    Ok(())
                }
            },
        })
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let src = names(from)?;
        let dst = names(to)?;
        self.with(|root| {
            let src_kind = kind_of_node(lookup(root, &src)?);
            if src.is_empty() || dst.is_empty() {
                return Err(if src.is_empty() { einval() } else { eexist() });
            }
            // the destination's directory must exist
            match lookup(root, &dst[..dst.len() - 1])? {
                Node::Dir(_) => {}
                Node::File(_) => return Err(enotdir()),
            }
            if src == dst {
                return Ok(());
            }
            if src_kind == Kind::Dir && dst.starts_with(&src) {
                return Err(einval());
            }
            match lookup(root, &dst) {
                Ok(Node::Dir(children)) => match src_kind {
                    Kind::Dir if !children.is_empty() => return Err(enotempty()),
                    Kind::Dir => {}
                    _ => return Err(eisdir()),
                },
                Ok(Node::File(_)) if src_kind == Kind::Dir => return Err(enotdir()),
                _ => {}
            }
            let node = match parent_mut(root, &src)? {
                Some((dir, name)) => dir.remove(name).ok_or_else(enoent)?,
                None => return Err(einval()),
            };
            match parent_mut(root, &dst)? {
                Some((dir, name)) => {
                    dir.insert(name.clone(), node);
                    Ok(())
                }
                None => Err(eexist()),
            }
        })
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<io::Result<Entry>>> {
        let names = names(path)?;
        self.with(|root| match lookup(root, &names)? {
            Node::File(_) => Err(enotdir()),
            Node::Dir(children) => Ok(children
                .iter()
                .map(|(name, node)| Ok(Entry { name: name.clone(), kind: Ok(kind_of_node(node)) }))
                .collect()),
        })
    }
    fn copy_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        let src = names(from)?;
        let bytes = self.with(|root| match lookup(root, &src)? {
            Node::File(b) => Ok(Some(b.clone())),
            // opening a directory works; reading it fails, after the
            // destination was opened (as std::fs::copy does)
            Node::Dir(_) => Ok(None),
        })?;
        match bytes {
            Some(bytes) => self.write(to, &bytes),
            None => {
                self.write(to, &[])?;
                Err(eisdir())
            }
        }
    }
}

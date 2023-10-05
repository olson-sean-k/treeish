mod parse;

#[cfg(feature = "miette")]
use miette::Diagnostic;
use std::borrow::Cow;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use thiserror::Error;
use wax::{CandidatePath, Glob, Walk, WalkBehavior};

use crate::parse::{ParseError, Partitioned};

// A treeish uses the following syntax:
//
// `C:\Users::**/*.txt`
// `\\.\COM1::**/*.txt`
// `\\?\UNC\server\share::**/*.txt`
// `/mnt/media1::**/*.txt`
//
// This uses `::` as the separator. Consider `>>`.

trait Empty {
    fn is_empty(&self) -> bool;

    fn non_empty(self) -> Option<Self>
    where
        Self: Sized,
    {
        if self.is_empty() {
            None
        }
        else {
            Some(self)
        }
    }
}

impl<'t> Empty for Glob<'t> {
    fn is_empty(&self) -> bool {
        // TODO: Gross!
        self.to_string().is_empty()
    }
}

impl<'p> Empty for &'p Path {
    fn is_empty(&self) -> bool {
        self.as_os_str().is_empty()
    }
}

impl Empty for PathBuf {
    fn is_empty(&self) -> bool {
        self.as_path().is_empty()
    }
}

#[derive(Debug, Error)]
#[error(transparent)]
#[cfg_attr(feature = "miette", derive(Diagnostic))]
pub struct BuildError {
    kind: BuildErrorKind,
}

impl From<wax::BuildError> for BuildError {
    fn from(error: wax::BuildError) -> Self {
        BuildError { kind: error.into() }
    }
}

impl<'t> From<ParseError<'t>> for BuildError {
    fn from(error: ParseError) -> Self {
        BuildError { kind: error.into() }
    }
}
impl From<RuleError> for BuildError {
    fn from(error: RuleError) -> Self {
        BuildError { kind: error.into() }
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
#[cfg_attr(feature = "miette", derive(Diagnostic))]
enum BuildErrorKind {
    #[error(transparent)]
    Glob(wax::BuildError),
    #[error(transparent)]
    Parse(ParseError<'static>),
    #[error(transparent)]
    Rule(RuleError),
}

impl From<wax::BuildError> for BuildErrorKind {
    fn from(error: wax::BuildError) -> Self {
        BuildErrorKind::Glob(error)
    }
}

impl<'t> From<ParseError<'t>> for BuildErrorKind {
    fn from(error: ParseError<'t>) -> Self {
        BuildErrorKind::Parse(error.into_owned())
    }
}

impl From<RuleError> for BuildErrorKind {
    fn from(error: RuleError) -> Self {
        BuildErrorKind::Rule(error)
    }
}

#[derive(Debug, Error)]
#[cfg_attr(feature = "miette", derive(Diagnostic))]
enum RuleError {
    #[error("")]
    RootedPatternIn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Unrooted<T>(T);

impl<T> Unrooted<T> {
    fn map<U, F>(self, f: F) -> Unrooted<U>
    where
        F: FnOnce(T) -> U,
    {
        Unrooted(f(self.0))
    }
}

impl<'t> Unrooted<TreeishGlob<'t>> {
    pub fn into_owned(self) -> Unrooted<TreeishGlob<'static>> {
        self.map(TreeishGlob::into_owned)
    }
}

impl<T> AsRef<T> for Unrooted<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for Unrooted<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct TreeishPath<'p>(Cow<'p, Path>);

impl<'p> TreeishPath<'p> {
    pub fn into_owned(self) -> TreeishPath<'static> {
        TreeishPath(self.0.into_owned().into())
    }
}

impl<'p> AsRef<Path> for TreeishPath<'p> {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

impl<'p> Deref for TreeishPath<'p> {
    type Target = Cow<'p, Path>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'p> From<TreeishPath<'p>> for Cow<'p, Path> {
    fn from(path: TreeishPath<'p>) -> Self {
        path.0
    }
}

#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct TreeishGlob<'t>(Glob<'t>);

impl<'t> TreeishGlob<'t> {
    pub fn into_owned(self) -> TreeishGlob<'static> {
        TreeishGlob(self.0.into_owned())
    }
}

impl<'t> AsRef<Glob<'t>> for TreeishGlob<'t> {
    fn as_ref(&self) -> &Glob<'t> {
        &self.0
    }
}

impl<'t> Deref for TreeishGlob<'t> {
    type Target = Glob<'t>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'t> From<TreeishGlob<'t>> for Glob<'t> {
    fn from(glob: TreeishGlob<'t>) -> Self {
        glob.0
    }
}

// TODO: What about "empty"? Arguably, path-like structures should not provide an intrinsic
//       representation of empty, like `Path("")`. Instead, an extrinsic representation like `None`
//       should be used and types like `Path` should **never** represent "nothing". `Treeish`
//       should do the same, and APIs should probably be fallible (or if not conceptually
//       "fallible" still yield `Option<Treeish>`).
pub enum Treeish<'t> {
    Path(TreeishPath<'t>),
    Glob(TreeishGlob<'t>),
    GlobIn {
        tree: TreeishPath<'t>,
        glob: Unrooted<TreeishGlob<'t>>,
    },
}

impl<'t> Treeish<'t> {
    fn empty() -> Treeish<'static> {
        // TODO: Gross.
        Treeish::Path(TreeishPath(Path::new("").into()))
    }

    pub fn new(expression: &'t str) -> Result<Self, BuildError> {
        parse::parse(expression)?.try_into()
    }

    pub fn into_owned(self) -> Treeish<'static> {
        use Treeish::{Glob, GlobIn, Path};

        match self {
            Path(path) => Path(path.into_owned()),
            Glob(glob) => Glob(glob.into_owned()),
            GlobIn { tree, glob } => GlobIn {
                tree: tree.into_owned(),
                glob: glob.into_owned(),
            },
        }
    }

    pub fn walk(&self) -> Walk {
        self.walk_with_behavior(WalkBehavior::default())
    }

    pub fn walk_with_behavior(&self, behavior: impl Into<WalkBehavior>) -> Walk {
        match self {
            Treeish::Path(ref path) => {
                let glob = Glob::new("").unwrap();
                glob.walk_with_behavior(path.as_ref(), behavior)
                    .into_owned()
            },
            // TODO: `.` isn't truly cross-platform.
            Treeish::Glob(ref glob) => glob.walk_with_behavior(".", behavior),
            Treeish::GlobIn { ref tree, ref glob } => {
                glob.walk_with_behavior(tree.as_ref(), behavior)
            },
        }
    }

    pub fn is_semantic_match<'p>(&self, path: impl Into<CandidatePath<'p>>) -> bool {
        let _path = path.into();
        todo!()
    }

    pub fn path(self) -> Option<Cow<'t, Path>> {
        match self {
            Treeish::Path(TreeishPath(path)) => Some(path),
            _ => None,
        }
    }

    pub fn glob(self) -> Option<Glob<'t>> {
        match self {
            Treeish::Glob(TreeishGlob(glob)) => Some(glob),
            _ => None,
        }
    }

    pub fn glob_in(self) -> Option<(Cow<'t, Path>, Glob<'t>)> {
        match self {
            Treeish::GlobIn {
                tree: TreeishPath(tree),
                glob: Unrooted(TreeishGlob(glob)),
            } => Some((tree, glob)),
            _ => None,
        }
    }

    pub fn has_path(&self) -> bool {
        matches!(self, Treeish::Path(_))
    }

    pub fn has_glob(&self) -> bool {
        matches!(self, Treeish::Glob(_) | Treeish::GlobIn { .. })
    }
}

impl<'t> From<&'t Path> for Treeish<'t> {
    fn from(path: &'t Path) -> Self {
        // TODO: May be empty! Gross.
        Treeish::Path(TreeishPath(path.into()))
    }
}

impl<'t> TryFrom<Glob<'t>> for Treeish<'t> {
    type Error = BuildError;

    fn try_from(glob: Glob<'t>) -> Result<Self, Self::Error> {
        // TODO: `Glob::partition` presents a bad interface here, because it does not cleanly
        //       indicate which parts are meaningful (non-empty). Change this upstream.
        let (path, glob) = glob.partition();
        Treeish::try_from(match (path.non_empty(), glob.non_empty()) {
            (Some(path), None) => Some(Partitioned::Path(path.into())),
            (None, Some(glob)) => Some(Partitioned::Glob(glob)),
            (Some(path), Some(glob)) => Some(Partitioned::GlobIn(path.into(), glob)),
            (None, None) => None,
        })
    }
}

// This conversion implements rules that must be checked before constructing a `Treeish`.
impl<'t> TryFrom<Option<Partitioned<'t>>> for Treeish<'t> {
    type Error = BuildError;

    fn try_from(partitioned: Option<Partitioned<'t>>) -> Result<Self, Self::Error> {
        if let Some(partitioned) = partitioned {
            match partitioned {
                Partitioned::Path(path) => Ok(Treeish::Path(TreeishPath(path))),
                Partitioned::Glob(glob) => Ok(Treeish::Glob(TreeishGlob(glob))),
                Partitioned::GlobIn(path, glob) => {
                    if glob.has_root() {
                        // TODO: Provide details in the error.
                        // If the glob still has a root, then it cannot be joined to a native path
                        // non-destructively. Such treeish expressions are not allowed.
                        Err(RuleError::RootedPatternIn.into())
                    }
                    else {
                        Ok(Treeish::GlobIn {
                            tree: TreeishPath(path),
                            glob: Unrooted(TreeishGlob(glob)),
                        })
                    }
                },
            }
        }
        else {
            // TODO: Gross.
            Ok(Treeish::empty())
        }
    }
}

#[cfg(test)]
mod tests {}

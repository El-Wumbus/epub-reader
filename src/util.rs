#![allow(dead_code)]
pub use self::either::Either;

use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
};

pub const KIB: usize = 1024;
pub const MIB: usize = KIB * 1024;
pub const GIB: usize = MIB * 1024;
pub const TIB: usize = GIB * 1024;

#[macro_export]
macro_rules! pred_cmp {
    ($optional:expr, $comparison:expr) => {
        ($optional.is_none() || $optional.as_ref().is_some_and($comparison))
    };
}

pub trait StringUtil
where
    Self: AsRef<str>,
{
    fn concat(&self, other: &Self) -> String {
        let mut new = String::from(self.as_ref());
        new.push_str(other.as_ref());
        new
    }

    /// Left-justifies the input string within a field of specified length by
    /// adding pad character.
    fn left_justify(&self, padding: char, length: usize) -> String {
        let s = self.as_ref();
        let n = s.chars().count();

        if n >= length || padding == '\0' {
            return s.to_string();
        }

        format!("{s}{}", padding.to_string().repeat(length - n))
    }

    /// Compute the Levenshtein edit distance between two strings.  
    /// NOTE: Does not perform internal allocation if length of string `b`, in
    ///       runes, is smaller than 64.  
    /// NOTE: This implementation is a single-row-version of the Wagner–Fischer
    ///       algorithm. This is based on the Odin core library implementation,
    ///       which was based on C code by Martin Ettl.
    #[allow(clippy::needless_range_loop)]
    fn levenshtein_distance(&self, other: &Self) -> usize {
        let mut levenshtein_default_costs: [usize; 64] = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, //
            10, 11, 12, 13, 14, 15, 16, 17, 18, 19, //
            20, 21, 22, 23, 24, 25, 26, 27, 28, 29, //
            30, 31, 32, 33, 34, 35, 36, 37, 38, 39, //
            40, 41, 42, 43, 44, 45, 46, 47, 48, 49, //
            50, 51, 52, 53, 54, 55, 56, 57, 58, 59, //
            60, 61, 62, 63, //
        ];

        let a = self.as_ref();
        let b = other.as_ref();
        let m = a.chars().count();
        let n = b.chars().count();

        if m == 0 {
            return n;
        }
        if n == 0 {
            return m;
        }

        let costs: &mut [usize];
        let mut c: Vec<_>;
        if n + 1 > levenshtein_default_costs.len() {
            c = Vec::with_capacity(n + 1);
            costs = &mut c;
            for k in 0..=n {
                costs[k] = k;
            }
        } else {
            costs = &mut levenshtein_default_costs;
        }

        for (i, c1) in a.chars().enumerate() {
            costs[0] = i + 1;
            let mut corner = i;
            for (j, c2) in b.chars().enumerate() {
                let upper = costs[j + 1];
                if c1 == c2 {
                    costs[j + 1] = corner;
                } else {
                    let t = if upper < corner { upper } else { corner };
                    costs[j + 1] = (if costs[j] < t { costs[j] } else { t }) + 1;
                }
                corner = upper;
            }
        }

        costs[n]
    }
}

impl<S> StringUtil for S where S: AsRef<str> {}

pub trait ShitStringUtil
where
    Self: AsRef<OsStr>,
{
    fn concat(&self, strs: &[&dyn AsRef<OsStr>]) -> OsString {
        let base = self.as_ref();
        let strs = strs.iter().map(|x| x.as_ref());
        let strs_total_size: usize = strs.clone().map(|x| x.len_bytes()).sum();

        let mut bytes = Vec::with_capacity(base.len_bytes() + strs_total_size);
        bytes.extend_from_slice(base.as_encoded_bytes());
        for s in strs {
            bytes.extend_from_slice(s.as_encoded_bytes());
        }
        OsString::from_vec(bytes)
    }
}

impl<S: AsRef<OsStr>> ShitStringUtil for S {}

pub trait PathUtil
where
    Self: AsRef<Path>,
{
    fn walk(
        &self,
        callback: &mut dyn FnMut(&std::path::Path) -> Result<(), std::io::Error>,
    ) -> Result<(), std::io::Error> {
        let dir = self.as_ref();
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    path.walk(callback)?;
                } else {
                    callback(&path)?;
                }
            }
        } else {
            // We don't want to ignore the first item if it's a file
            callback(dir)?;
        }
        Ok(())
    }

    /// Recursively walk `self`, summing each file's size. If `self` is a file,
    /// it returns `self`'s size.
    fn dir_size(&self) -> Result<usize, std::io::Error> {
        let mut size = 0usize;

        self.walk(&mut |entry| {
            size += entry.metadata()?.len() as usize;
            Ok(())
        })?;

        Ok(size)
    }

    /// Recursively copy a directory's files (`self`) into another directory
    /// (`to`). Does not copy empty directories.
    fn copy_dir(&self, to: &Self) -> Result<(), std::io::Error> {
        let from = self.as_ref();
        let todir = to.as_ref();

        if !std::fs::metadata(from)?.is_dir() {
            let filepath = todir.join(from.file_name().unwrap());
            std::fs::copy(from, filepath)?;
            return Ok(());
        }

        from.walk(&mut |item| {
            let filepath = todir.join(item.strip_prefix(from).unwrap());
            if let Some(parent) = filepath.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            std::fs::copy(item, &filepath)?;
            Ok(())
        })?;

        Ok(())
    }

    fn len(&self) -> usize {
        self.as_ref().as_os_str().len()
    }
    fn len_bytes(&self) -> usize {
        self.as_ref().as_os_str().as_encoded_bytes().len()
    }

    fn find(&self, pat: impl AsRef<str>) -> Option<usize> {
        let pat = pat.as_ref();
        let bytes = self.as_ref().as_os_str().as_encoded_bytes();
        for (i, window) in bytes.windows(pat.len()).enumerate() {
            if window == pat.as_bytes() {
                return Some(i);
            }
        }
        None
    }

    fn contains(&self, pat: impl AsRef<str>) -> bool {
        self.find(pat).is_some()
    }

    fn raw_concat(&self, strs: &[&dyn AsRef<Path>]) -> PathBuf {
        let base = self.as_ref();
        let strs = strs.iter().map(|x| x.as_ref().as_os_str());
        let strs_total_size: usize = strs.clone().map(|x| x.len_bytes()).sum();

        let mut bytes = Vec::with_capacity(base.len_bytes() + strs_total_size);
        bytes.extend_from_slice(base.as_os_str().as_encoded_bytes());
        for s in strs {
            bytes.extend_from_slice(s.as_encoded_bytes());
        }
        PathBuf::from(OsString::from_vec(bytes))
    }

    // fn concat_strings(&self, strs: &[impl AsRef<str>]) -> PathBuf {
    //     let base = self.as_ref();
    //     let strs = strs.into_iter().map(|x| x.as_ref());
    //     let strs_total_size: usize = strs.clone().map(str::len).sum();

    //     let mut bytes = Vec::with_capacity(base.len_bytes() + strs_total_size);
    //     bytes.extend_from_slice(base.as_os_str().as_encoded_bytes());
    //     for s in strs {
    //         bytes.extend_from_slice(s.as_bytes());
    //     }
    //     PathBuf::from(OsString::from_vec(bytes))
    // }
}

impl<S> PathUtil for S where S: AsRef<Path> {}

pub trait ByteUitil {
    fn to_human_string(self) -> String;
}

impl ByteUitil for usize {
    fn to_human_string(self) -> String {
        let size = self as f64;
        let (count, label) = if self < KIB {
            return format!("{self} B)");
        } else if self < MIB {
            (size / KIB as f64, "KiB")
        } else if self < GIB {
            (size / MIB as f64, "MiB")
        } else if self < TIB {
            (size / GIB as f64, "GiB")
        } else {
            (size / TIB as f64, "TiB")
        };
        format!("{count:.2} {label}")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn paths() {
        let x = PathBuf::from("HELP");
        assert!(x.len() == 4);
        assert!(x.find("EL") == Some(1));
        assert!(x.contains("LP"));
    }

    #[test]
    fn byte() {
        assert!((20 * KIB).to_human_string() == "20.00 KiB");
    }
}

pub mod either {
    pub use Either::{Left, Right};
    #[derive(Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub enum Either<L, R> {
        /// A value of type `L`.
        Left(L),
        /// A value of type `R`.
        Right(R),
    }

    impl<L: Clone, R: Clone> Clone for Either<L, R> {
        fn clone(&self) -> Self {
            match self {
                Left(inner) => Left(inner.clone()),
                Right(inner) => Right(inner.clone()),
            }
        }

        fn clone_from(&mut self, source: &Self) {
            match (self, source) {
                (Left(dest), Left(source)) => dest.clone_from(source),
                (Right(dest), Right(source)) => dest.clone_from(source),
                (dest, source) => *dest = source.clone(),
            }
        }
    }

    impl<L, R> Either<L, R> {
        /// Return true if the value is the `Left` variant.
        ///
        /// ```
        /// use either::*;
        ///
        /// let values = [Left(1), Right("the right value")];
        /// assert_eq!(values[0].is_left(), true);
        /// assert_eq!(values[1].is_left(), false);
        /// ```
        pub fn is_left(&self) -> bool {
            match *self {
                Left(_) => true,
                Right(_) => false,
            }
        }

        /// Return true if the value is the `Right` variant.
        ///
        /// ```
        /// use either::*;
        ///
        /// let values = [Left(1), Right("the right value")];
        /// assert_eq!(values[0].is_right(), false);
        /// assert_eq!(values[1].is_right(), true);
        /// ```
        pub fn is_right(&self) -> bool {
            !self.is_left()
        }

        /// Convert the left side of `Either<L, R>` to an `Option<L>`.
        ///
        /// ```
        /// use either::*;
        ///
        /// let left: Either<_, ()> = Left("some value");
        /// assert_eq!(left.left(),  Some("some value"));
        ///
        /// let right: Either<(), _> = Right(321);
        /// assert_eq!(right.left(), None);
        /// ```
        pub fn left(self) -> Option<L> {
            match self {
                Left(l) => Some(l),
                Right(_) => None,
            }
        }

        /// Convert the right side of `Either<L, R>` to an `Option<R>`.
        ///
        /// ```
        /// use either::*;
        ///
        /// let left: Either<_, ()> = Left("some value");
        /// assert_eq!(left.right(),  None);
        ///
        /// let right: Either<(), _> = Right(321);
        /// assert_eq!(right.right(), Some(321));
        /// ```
        pub fn right(self) -> Option<R> {
            match self {
                Left(_) => None,
                Right(r) => Some(r),
            }
        }
    }
}

pub mod path {
    /// From <https://github.com/rescrv/blue/blob/main/utf8path>.
    /// Modifications made
    use std::borrow::{Borrow, Cow};
    use std::path::PathBuf;

    /////////////////////////////////////////////// Path ///////////////////////////////////////////////

    /// Path provides a copy-on-write-style path that is built around UTF8 strings.
    #[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
    pub struct Path<'a> {
        path: Cow<'a, str>,
    }

    impl<'a> Path<'a> {
        /// Create a new path that borrows the provided string.
        pub const fn new(s: &'a str) -> Self {
            Self {
                path: Cow::Borrowed(s),
            }
        }

        /// Convert the path into an owned path.
        pub fn into_owned(self) -> Path<'static> {
            Path {
                path: Cow::Owned(self.path.into_owned()),
            }
        }

        /// Convert the path into a std::path::PathBuf.
        pub fn into_std(&self) -> &std::path::Path {
            std::path::Path::new::<str>(self.path.as_ref())
        }

        /// Convert the path to a str.
        pub fn as_str(&self) -> &str {
            &self.path
        }

        /// Is the path a directory?
        pub fn is_dir(&self) -> bool {
            std::path::Path::new(self.path.as_ref()).is_dir()
        }

        /// Compute the basename of the path.  This is guaraneed to be a non-empty path component
        /// (falling back to "." for paths that end with "/").
        pub fn basename(&self) -> Path<'_> {
            self.split().1
        }

        /// Compute the dirname of the path.  This is guaranteed to be a non-empty path component
        /// (falling back to "." or "/" for single-component paths).
        pub fn dirname(&self) -> Path<'_> {
            self.split().0
        }

        /// True if the path exists.
        pub fn exists(&self) -> bool {
            let path: &str = &self.path;
            PathBuf::from(path).exists()
        }

        /// True if the path begins with some number of slashes, other than the POSIX-exception of //.
        pub fn has_root(&self) -> bool {
            self.path.starts_with('/') && !self.has_app_defined()
        }

        /// True if the path begins with //, but not ///.
        pub fn has_app_defined(&self) -> bool {
            self.path.starts_with("//") && (self.path.len() == 2 || &self.path[2..3] != "/")
        }

        /// True if the path is absolute.
        pub fn is_abs(&self) -> bool {
            self.has_root() || self.has_app_defined()
        }

        /// True if the path contains no "." components; and, is absolute and has no ".." components,
        /// or is relative and has all ".." components at the start.
        pub fn is_normal(&self) -> bool {
            let start = if self.path.starts_with("//") {
                2
            } else if self.path.starts_with('/') {
                1
            } else {
                0
            };
            if self.path[start..].is_empty() {
                return start > 0;
            }
            let limit = if self.path[start..].ends_with('/') {
                self.path.len() - 1
            } else {
                self.path.len()
            };
            let components: Vec<_> = self.path[start..limit].split('/').collect();
            let mut parent_allowed = start == 0;
            for component in components {
                if parent_allowed {
                    if matches!(component, "." | "") {
                        return false;
                    }
                    parent_allowed = component == "..";
                } else if matches!(component, ".." | "." | "") {
                    return false;
                }
            }
            true
        }

        /// Join to this path another path.  Follows standard path rules where if the joined-with path
        /// is absolute, the first path is discarded.
        pub fn join<'b, 'c>(&self, with: impl Into<Path<'b>>) -> Path<'c>
        where
            'a: 'c,
            'b: 'c,
        {
            let with = with.into();
            if with.is_abs() {
                with.clone()
            } else {
                Path::from(format!("{}/{}", self.path, with.path))
            }
        }

        /// Strip a prefix from the path.  The prefix and path are allowed to be non-normal and will
        /// have "." components dropped from consideration.
        pub fn strip_prefix<'b>(&self, prefix: impl Into<Path<'b>>) -> Option<Path> {
            let prefix = prefix.into();
            // NOTE(rescrv):  You might be tempted to use components() and zip() to solve and/or
            // simplify this.  That fails for one reason:  "components()" intentionally rewrites `foo/`
            // as `foo/.`, but this method should preserve the path that remains as much as possible,
            // including `.` components.
            if self.has_root() && !prefix.has_root() {
                return None;
            }
            if self.has_app_defined() && !prefix.has_app_defined() {
                return None;
            }
            let mut path = self.path[..].trim_start_matches('/');
            let mut prefix = prefix.path[..].trim_start_matches('/');
            loop {
                if let Some(prefix_slash) = prefix.find('/') {
                    let path_slash = path.find('/')?;
                    if prefix[..prefix_slash] != path[..path_slash] {
                        return None;
                    }
                    path = path[path_slash + 1..].trim_start_matches('/');
                    prefix = prefix[prefix_slash + 1..].trim_start_matches('/');
                } else if prefix == path {
                    return Some(Path::new("."));
                } else if let Some(path) = path.strip_prefix(prefix) {
                    let path = path.trim_start_matches('/');
                    if path.is_empty() {
                        return Some(Path::new("."));
                    } else {
                        return Some(Path::new(path));
                    }
                } else if prefix.starts_with("./") {
                    prefix = prefix[2..].trim_start_matches('/');
                } else if path.starts_with("./") {
                    path = path[2..].trim_start_matches('/');
                } else if prefix.is_empty() || prefix == "." {
                    if path.is_empty() {
                        return Some(Path::new("."));
                    } else {
                        return Some(Path::new(path));
                    }
                }
            }
        }

        /// Split the path into basename and dirname components.
        pub fn split(&self) -> (Path, Path) {
            if let Some(index) = self.path.rfind('/') {
                let dirname = if index == 0 {
                    Path::new("/")
                } else if index == 1 && self.path.starts_with("//") {
                    Path::new("//")
                } else if self.path[..index].chars().all(|c| c == '/') {
                    Path::new("/")
                } else {
                    Path::new(self.path[..index].trim_end_matches('/'))
                };
                let basename = if index + 1 == self.path.len() {
                    Path::new(".")
                } else {
                    Path::new(&self.path[index + 1..])
                };
                (dirname, basename)
            } else {
                (Path::new("."), Path::new(&self.path))
            }
        }

        /// Return an iterator ovre the path components.  A path with a basename of "." will always end
        /// with Component::CurDir.
        pub fn components(&self) -> impl Iterator<Item = Component<'_>> {
            let mut components = vec![];
            let mut limit = self.path.len();
            while let Some(slash) = self.path[..limit].rfind('/') {
                if slash + 1 == limit {
                    components.push(Component::CurDir);
                } else if &self.path[slash + 1..limit] == ".." {
                    components.push(Component::ParentDir);
                } else if &self.path[slash + 1..limit] == "." {
                    components.push(Component::CurDir);
                } else {
                    components.push(Component::Normal(Path::new(&self.path[slash + 1..limit])));
                }
                if slash == 0 {
                    components.push(Component::RootDir);
                    limit = 0;
                } else if slash == 1 && self.path.starts_with("//") {
                    components.push(Component::AppDefined);
                    limit = 0;
                } else if self.path[..slash].chars().all(|c| c == '/') {
                    components.push(Component::RootDir);
                    limit = 0;
                } else {
                    limit = slash;
                    while limit > 0 && self.path[..limit].ends_with('/') {
                        limit -= 1;
                    }
                }
            }
            if limit > 0 {
                if &self.path[..limit] == ".." {
                    components.push(Component::ParentDir);
                } else if &self.path[..limit] == "." {
                    components.push(Component::CurDir);
                } else {
                    components.push(Component::Normal(Path::new(&self.path[..limit])));
                }
            }
            components.reverse();
            components.into_iter()
        }

        /// Return the current working directory, if it can be fetched and converted to unicode without
        /// error.
        pub fn cwd() -> Option<Path<'a>> {
            Path::try_from(std::env::current_dir().ok()?).ok()
        }
    }

    impl<'a> AsRef<std::ffi::OsStr> for Path<'a> {
        fn as_ref(&self) -> &std::ffi::OsStr {
            let path: &std::ffi::OsStr = self.as_str().as_ref();
            path
        }
    }

    impl<'a> AsRef<std::path::Path> for Path<'a> {
        fn as_ref(&self) -> &std::path::Path {
            std::path::Path::new(self.as_str())
        }
    }

    impl<'a> Borrow<std::path::Path> for Path<'a> {
        fn borrow(&self) -> &std::path::Path {
            std::path::Path::new(self.as_str())
        }
    }

    impl<'a> std::fmt::Debug for Path<'a> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
            write!(f, "{:?}", self.path)
        }
    }

    impl<'a> std::fmt::Display for Path<'a> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
            write!(f, "{}", self.path)
        }
    }

    impl<'a> From<String> for Path<'a> {
        fn from(s: String) -> Self {
            Self {
                path: Cow::Owned(s),
            }
        }
    }

    impl<'a> From<Path<'a>> for String {
        fn from(path: Path<'a>) -> Self {
            path.path.into_owned()
        }
    }

    impl<'a> From<&'a String> for Path<'a> {
        fn from(s: &'a String) -> Self {
            Self {
                path: Cow::Borrowed(s),
            }
        }
    }

    impl<'a> From<&'a str> for Path<'a> {
        fn from(s: &'a str) -> Self {
            Self {
                path: Cow::Borrowed(s),
            }
        }
    }

    impl<'a> From<&'a Path<'a>> for &'a str {
        fn from(path: &'a Path<'a>) -> Self {
            &path.path
        }
    }

    impl<'a> TryFrom<&'a std::path::Path> for Path<'a> {
        type Error = std::str::Utf8Error;

        fn try_from(p: &'a std::path::Path) -> Result<Self, Self::Error> {
            Ok(Self {
                path: Cow::Borrowed(<&str>::try_from(p.as_os_str())?),
            })
        }
    }

    impl<'a> TryFrom<std::path::PathBuf> for Path<'a> {
        type Error = std::str::Utf8Error;

        fn try_from(p: std::path::PathBuf) -> Result<Self, Self::Error> {
            Ok(Self {
                path: Cow::Owned(<&str>::try_from(p.as_os_str())?.to_string()),
            })
        }
    }

    impl<'a> TryFrom<std::ffi::OsString> for Path<'a> {
        type Error = std::str::Utf8Error;

        fn try_from(p: std::ffi::OsString) -> Result<Self, Self::Error> {
            Ok(Self {
                path: Cow::Owned(<&str>::try_from(p.as_os_str())?.to_string()),
            })
        }
    }

    impl<'a> From<Path<'a>> for std::path::PathBuf {
        fn from(path: Path<'a>) -> Self {
            PathBuf::from(path.path.to_string())
        }
    }

    ///////////////////////////////////////////// Component ////////////////////////////////////////////

    /// A component of a path.
    #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
    pub enum Component<'a> {
        /// Signals the path component "/".
        RootDir,
        /// Signals the path component "//".
        AppDefined,
        /// Signals the "." path component.
        CurDir,
        /// Signals the ".." path component.
        ParentDir,
        /// Signals a component that doesn't match any of the special components.
        Normal(Path<'a>),
    }
}

pub mod mime {
    pub const XHTML: &str = "application/xhtml+xml";
    pub const HTML: &str = "text/html";
    pub const JSON: &str = "application/json";
    pub const CSS: &str = "text/css";
}

//! Query system for default apps using XDG MIME databases.
//!
//! The xdg-utils library provides dependency-free (except for `std`) Rust implementations of some
//! common functions in the freedesktop project `xdg-utils`.
//!
//! # What is implemented?
//! * Function [`query_default_app`] performs like the xdg-utils function `binary_to_desktop_file`
//!
//! Some of the utils may be implemented by combining these functions with other functions in the Rust
//! standard library.
//!
//! | Name            | Function                                               | Implemented functionalities|
//! |-----------------|--------------------------------------------------------|----------------------------|
//! |`xdg-desktop-menu`| Install desktop menu items                             | no
//! |`xdg-desktop-icon`| Install icons to the desktop                           | no
//! |`xdg-icon-resource`| Install icon resources                                 | no
//! |`xdg-mime`        | Query information about file type handling and install descriptions for new file types| queries only
//! |`xdg-open`        | Open a file or URL in the user's preferred application | all (combine crate functions with `std::process::Command`)
//! |`xdg-email`       | Send mail using the user's preferred e-mail composer   | no
//! |`xdg-screensaver` | Control the screensaver                                | no
//!
//! # Specification
//! <https://specifications.freedesktop.org/mime-apps-spec/mime-apps-spec-latest.html>
//!
//! # Reference implementation
//! <https://cgit.freedesktop.org/xdg/xdg-utils/tree/scripts/xdg-utils-common.in>


use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

macro_rules! split_and_chain {
    ($xdg_vars:ident[$key:literal]) => {
        $xdg_vars.get($key).map(String::as_str).unwrap_or("").split(':')
    };
    ($xdg_vars:ident[$key:literal], $($tail_xdg_vars:ident[$tail_key:literal]),+$(,)*) => {

        split_and_chain!($xdg_vars[$key]).chain(split_and_chain!($($tail_xdg_vars[$tail_key]),+))
    }
}

/// A simple zero-copy INI parser.
#[derive(Debug)]
struct INIParser<'a> {
    lines: std::str::Lines<'a>,
    section: &'a str,
}

#[derive(Debug, Clone, Copy)]
struct INIPair<'a> {
    section: &'a str,
    key: &'a str,
    value: &'a str,
}

impl<'a> INIParser<'a> {
    fn next_pair(&mut self) -> Option<INIPair<'a>> {
        while let Some(line) = self.lines.next() {
            let line = line.trim();
            if line.starts_with("#") {
                continue;
            } else if line.starts_with("[") && line.ends_with("]") {
                self.section = line.get(1..line.len() - 1).unwrap();
            } else if let Some((key, value)) = line.split_once('=') {
                return Some(INIPair {
                    section: self.section,
                    key,
                    value,
                });
            }
        }
        None
    }
}

impl<'a> From<&'a str> for INIParser<'a> {
    fn from(s: &'a str) -> Self {
        Self {
            lines: s.lines(),
            section: "",
        }
    }
}

impl<'a> Iterator for INIParser<'a> {
    type Item = INIPair<'a>;

    fn next(&mut self) -> Option<INIPair<'a>> {
        self.next_pair()
    }
}

/// Returns the command string of the desktop file that is the default application of given MIME type `query`
///
/// # Example
/// ```no_run
/// use xdg_utils::query_default_app;
///
/// assert_eq!(Ok("firefox".into()), query_default_app("text/html").map_err(|_| ()));
/// ```
pub fn query_default_app<T: AsRef<str>>(query: T) -> std::io::Result<String> {
    // Values are directory paths separated by : in case it's more than one.
    let mut xdg_vars: HashMap<String, String> = HashMap::new();

    for (key, val) in std::env::vars() {
        if key.starts_with("XDG_CONFIG")
            || key.starts_with("XDG_DATA")
            || key.starts_with("XDG_CURRENT_DESKTOP")
            || key == "HOME"
        {
            xdg_vars.insert(key.to_string(), val.to_string());
        }
    }

    // Insert defaults if variables are missing
    if xdg_vars.contains_key("HOME") && !xdg_vars.contains_key("XDG_DATA_HOME")
    {
        let h = xdg_vars["HOME"].clone();
        xdg_vars
            .insert("XDG_DATA_HOME".to_string(), format!("{}/.local/share", h));
    }

    if xdg_vars.contains_key("HOME")
        && !xdg_vars.contains_key("XDG_CONFIG_HOME")
    {
        let h = xdg_vars["HOME"].clone();
        xdg_vars
            .insert("XDG_CONFIG_HOME".to_string(), format!("{}/.config", h));
    }

    if !xdg_vars.contains_key("XDG_DATA_DIRS") {
        xdg_vars.insert(
            "XDG_DATA_DIRS".to_string(),
            "/usr/local/share:/usr/share".to_string(),
        );
    }

    if !xdg_vars.contains_key("XDG_CONFIG_DIRS") {
        xdg_vars.insert("XDG_CONFIG_DIRS".to_string(), "/etc/xdg".to_string());
    }

    let desktops: Option<Vec<String>> =
        if xdg_vars.contains_key("XDG_CURRENT_DESKTOP") {
            let list = xdg_vars["XDG_CURRENT_DESKTOP"]
                .trim()
                .split(':')
                .map(str::to_ascii_lowercase)
                .collect();
            Some(list)
        } else {
            None
        };

    // Search for mime entry in files.
    for dir in split_and_chain!(
        xdg_vars["XDG_CONFIG_HOME"],
        xdg_vars["XDG_CONFIG_DIRS"],
        xdg_vars["XDG_DATA_HOME"],
        xdg_vars["XDG_DATA_DIRS"],
    ) {
        if let Some(ref d) = desktops {
            for desktop in d {
                let pb: PathBuf = PathBuf::from(format!(
                    "{var_value}/{desktop}-mimeapps.list",
                    var_value = dir,
                ));
                if pb.exists() {
                    if let Some(ret) =
                        check_mimeapps_list(&pb, &xdg_vars, &query)?
                    {
                        return Ok(ret);
                    }
                }
            }
        }
        let pb: PathBuf = PathBuf::from(format!(
            "{var_value}/mimeapps.list",
            var_value = dir
        ));
        println!("Looking for {}", pb.display());
        if pb.exists() {
            if let Some(ret) = check_mimeapps_list(&pb, &xdg_vars, &query)? {
                return Ok(ret);
            }
        }
    }

    // Search again but for different paths.
    for p in
        split_and_chain!(xdg_vars["XDG_DATA_HOME"], xdg_vars["XDG_DATA_DIRS"])
    {
        if let Some(ref d) = desktops {
            for desktop in d {
                let pb: PathBuf = PathBuf::from(format!(
                    "{var_value}/applications/{desktop_val}-mimeapps.list",
                    var_value = p,
                    desktop_val = desktop
                ));
                if pb.exists() {
                    if let Some(ret) =
                        check_mimeapps_list(&pb, &xdg_vars, &query)?
                    {
                        return Ok(ret);
                    }
                }
            }
        }
        let pb: PathBuf = PathBuf::from(format!(
            "{var_value}/applications/mimeapps.list",
            var_value = p
        ));
        if pb.exists() {
            if let Some(ret) = check_mimeapps_list(&pb, &xdg_vars, &query)? {
                return Ok(ret);
            }
        }
    }

    Err(Error::new(
        ErrorKind::NotFound,
        format!("No results for mime query: {}", query.as_ref()),
    ))
}

fn check_mimeapps_list<T: AsRef<str>>(
    filename: &Path,
    xdg_vars: &HashMap<String, String>,
    query: T,
) -> std::io::Result<Option<String>> {
    let ini = std::fs::read_to_string(filename)?;
    let ini = INIParser::from(ini.as_str()).filter(|x| {
        x.section == "Added Associations" || x.section == "Default Applications"
    });
    for INIPair { key, value, .. } in ini {
        println!("Found key: '{key}'");
        if key != query.as_ref() {
            continue;
        }
        for v in value.split(';') {
            if v.trim().is_empty() {
                continue;
            }

            if let Some(b) = desktop_file_to_command(v, xdg_vars)? {
                return Ok(Some(b));
            }
        }
    }

    Ok(None)
}


// Find the desktop file in the filesystem, then find the executable entry.
fn desktop_file_to_command(
    desktop_name: &str,
    xdg_vars: &HashMap<String, String>,
) -> std::io::Result<Option<String>> {
        // This is an absolute path, duhh.
        for dir in split_and_chain!(
            xdg_vars["XDG_DATA_HOME"],
            xdg_vars["XDG_DATA_DIRS"]
        ) {
            // Note from the second author: Don't blame me for this code, just blame me for fixing
            // it :).
            let mut file_path = if desktop_name.starts_with("/") {
                Some(PathBuf::from(desktop_name))
            } else { None };

            if file_path.is_none() && desktop_name.contains('-') {
                let v: Vec<&str> = desktop_name.split('-').collect();
                assert!(v.len() >= 2, "Expected `desktop_name` to contain at least two sections");
                let (vendor, app): (&str, &str) = (v[0], v[1]);
                // The code I adapted this from assumes the `desktop_name` has at least two
                // sections, so I assert this.

                let path = PathBuf::from(format!(
                    "{dir}/applications/{vendor}/{app}",
                ));
                if std::fs::exists(&path)? {
                    file_path = Some(path);
                }
            }

            if file_path.is_none() {
                'indir: for indir in &[format!("{}/applications", dir)] {
                    let mut path = PathBuf::from(indir).join(desktop_name);
                    if std::fs::exists(&path)? {
                        file_path = Some(path);
                        break 'indir;
                    }
                    path.pop(); // Remove {desktop} from path.
                    if path.is_dir() {
                        for entry in std::fs::read_dir(&path)? {
                            let mut path = entry?.path().to_owned();
                            path.push(desktop_name);
                            if path.exists() {
                                file_path = Some(path);
                                break 'indir;
                            }
                        }
                    }
                }
            }

            if let Some(file_path) = file_path {
                let ini = std::fs::read_to_string(&file_path)?;
                return Ok(INIParser::from(ini.as_str()).filter(|x| x.section == "Desktop Entry").find(|x| x.key == "Exec").map(|x|x.value.to_string()))
            }
        }

    Ok(None)
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        /* Run with `cargo test -- --nocapture` to see output. */
        println!("{:?}", query_default_app("image/jpeg"));
        println!("{:?}", query_default_app("text/html"));
        println!("{:?}", query_default_app("video/mp4"));
        println!("{:?}", query_default_app("application/pdf"));
    }

    #[test]
    fn ini2() {
        const INI: &str = r#"
        [foo]
        bar=baz
        # Comments at the start of lines
        [bar]
        baz=foo
        "#;

        let ini = INIParser::from(INI);
        let mut pairs = 0usize;
        for INIPair {
            section,
            key,
            value,
        } in ini
        {
            pairs += 1;
            match section {
                "foo" => {
                    assert_eq!(key, "bar");
                    assert_eq!(value, "baz");
                }
                "bar" => {
                    assert_eq!(key, "baz");
                    assert_eq!(value, "foo");
                }
                _ => panic!("Invalid section: \"{section}\""),
            }
        }
        assert_eq!(pairs, 2);
    }
}

use crate::lobby::ChannelID;
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf, str::Split};

/// A folder in the system
#[derive(Default, Debug, Deserialize)]
pub struct Folder {
    /// The directory to save the files to
    #[serde(default)]
    save_dir: Option<PathBuf>,

    /// The channels that are currently active
    #[serde(skip)]
    channels: HashMap<String, ChannelID>,

    /// The subfolders from this folder
    #[serde(default)]
    sub: HashMap<String, Folder>,
}

/// The type of file
pub enum PathValidity<'a, 'b> {
    Invalid,
    File(&'b mut Folder, PathBuf, &'a str),
    Folder(&'b mut Folder, PathBuf),
}

/// Checks the name for validity
impl Folder {
    fn check_name_iter<'a, 'b>(
        &'b mut self,
        mut iter: Split<'a, char>,
        curr: &'a str,
        mut base_dir: PathBuf,
    ) -> PathValidity<'a, 'b> {
        if let Some(base) = &self.save_dir {
            base_dir = base.clone();
        }
        match iter.next() {
            Some(next) => {
                // if there is a next file name
                if let Some(sub) = self.sub.get_mut(curr) {
                    base_dir.push(curr);
                    sub.check_name_iter(iter, next, base_dir)
                } else {
                    PathValidity::Invalid
                }
            }
            None if curr == "" => {
                // curr is the file name
                PathValidity::Folder(self, base_dir)
            }
            None => PathValidity::File(self, base_dir, curr),
        }
    }

    pub fn check_name<'a, 'b>(
        &'b mut self,
        path: &'a str,
        base_dir: PathBuf,
    ) -> PathValidity<'a, 'b> {
        let mut iter = path.split('/');
        if let Some("") = iter.next() {
            if let Some(curr) = iter.next() {
                self.check_name_iter(iter, curr, base_dir)
            } else {
                PathValidity::Invalid
            }
        } else {
            PathValidity::Invalid
        }
    }
}

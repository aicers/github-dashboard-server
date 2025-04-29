use std::{marker::PhantomData, path::Path};

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::Serialize;
use sled::{Db, Tree};

use crate::{
    github::{GitHubIssue, GitHubPullRequests},
    graphql::{Issue, PullRequest},
};
const ISSUE_TREE_NAME: &str = "issues";
const PULL_REQUEST_TREE_NAME: &str = "pull_requests";

#[derive(Clone)]
pub(crate) struct Database {
    db: Db,
    issue_tree: Tree,
    pull_request_tree: Tree,
}

impl Database {
    fn connect_db(path: &Path) -> Result<Db> {
        Ok(sled::open(path)?)
    }

    fn connect_trees(db: &Db) -> Result<(Tree, Tree)> {
        let issue_tree = db.open_tree(ISSUE_TREE_NAME)?;
        let pull_request_tree = db.open_tree(PULL_REQUEST_TREE_NAME)?;
        Ok((issue_tree, pull_request_tree))
    }

    pub(crate) fn connect(db_path: &Path) -> Result<Database> {
        let db = Database::connect_db(db_path)?;
        let (issue_tree, pull_request_tree) = Database::connect_trees(&db)?;
        Ok(Database {
            db,
            issue_tree,
            pull_request_tree,
        })
    }

    fn insert<T: Serialize>(key: &str, val: T, tree: &Tree) -> Result<()> {
        tree.insert(key, bincode::serialize(&val)?)?;
        Ok(())
    }

    pub(crate) fn insert_db<T: Serialize>(&self, key: &str, val: T) -> Result<()> {
        self.db.insert(key, bincode::serialize(&val)?)?;
        Ok(())
    }

    pub(crate) fn select_db(&self, key: &str) -> Result<String> {
        if let Ok(Some(val)) = self.db.get(key) {
            let result: String = bincode::deserialize(&val)?;
            return Ok(result);
        }
        bail!("Failed to get db value");
    }

    pub(crate) fn delete_db(&self, key: &str) -> Result<String> {
        if let Ok(Some(val)) = self.db.remove(key) {
            let result: String = bincode::deserialize(&val)?;
            return Ok(result);
        }
        bail!("Failed to remove tree value");
    }

    pub(crate) fn insert_issues(
        &self,
        resp: Vec<GitHubIssue>,
        owner: &str,
        name: &str,
    ) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{owner}/{name}#{}", item.number);
            Database::insert(
                &keystr,
                (&item.title, &item.author, &item.closed_at),
                &self.issue_tree,
            )?;
        }
        Ok(())
    }

    pub(crate) fn insert_pull_requests(
        &self,
        resp: Vec<GitHubPullRequests>,
        owner: &str,
        name: &str,
    ) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{owner}/{name}#{}", item.number);
            Database::insert(
                &keystr,
                (&item.title, &item.assignees, &item.reviewers),
                &self.pull_request_tree,
            )?;
        }
        Ok(())
    }

    pub(crate) fn issues(&self, start: Option<&[u8]>, end: Option<&[u8]>) -> Iter<Issue> {
        let start = start.unwrap_or(b"\x00");
        if let Some(end) = end {
            self.issue_tree.range(start..end).into()
        } else {
            self.issue_tree.range(start..).into()
        }
    }

    pub(crate) fn pull_requests(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Iter<PullRequest> {
        let start = start.unwrap_or(b"\x00");
        if let Some(end) = end {
            self.pull_request_tree.range(start..end).into()
        } else {
            self.pull_request_tree.range(start..).into()
        }
    }
}

pub(crate) trait TryFromKeyValue {
    fn try_from_key_value(key: &[u8], value: &[u8]) -> Result<Self>
    where
        Self: Sized;
}

pub(crate) struct Iter<T> {
    inner: sled::Iter,
    phantom: PhantomData<T>,
}

impl<T> From<sled::Iter> for Iter<T> {
    fn from(iter: sled::Iter) -> Self {
        Self {
            inner: iter,
            phantom: PhantomData,
        }
    }
}

impl<T: TryFromKeyValue> Iterator for Iter<T> {
    type Item = anyhow::Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|item| {
            let (key, value) = item?;
            T::try_from_key_value(&key, &value)
        })
    }
}

impl<T: TryFromKeyValue> DoubleEndedIterator for Iter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(|item| {
            let (key, value) = item?;
            T::try_from_key_value(&key, &value)
        })
    }
}

pub(crate) fn parse_key(key: &[u8]) -> Result<(String, String, i64)> {
    let re = Regex::new(r"(?P<owner>\S+)/(?P<name>\S+)#(?P<number>[0-9]+)").expect("valid regex");
    if let Some(caps) = re.captures(
        String::from_utf8(key.to_vec())
            .context("invalid key")?
            .as_str(),
    ) {
        let owner = caps
            .name("owner")
            .ok_or_else(|| anyhow!("invalid key"))?
            .as_str()
            .to_string();
        let name = caps
            .name("name")
            .ok_or_else(|| anyhow!("invalid key"))?
            .as_str()
            .to_string();
        let number = caps
            .name("number")
            .ok_or_else(|| anyhow!("invalid key"))?
            .as_str()
            .parse::<i64>()
            .context("invalid key")?;
        Ok((owner, name, number))
    } else {
        Err(anyhow!("invalid key"))
    }
}

mod tests {
    #[test]
    fn test_parse_key() {
        let key = "rust-lang/rust#12345";
        let (owner, name, number) = super::parse_key(key.as_bytes()).unwrap();
        assert_eq!(owner, "rust-lang");
        assert_eq!(name, "rust");
        assert_eq!(number, 12345);
    }
}

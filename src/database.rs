use std::{marker::PhantomData, path::Path};

use anyhow::{bail, Result};
use serde::Serialize;
use sled::{Db, Tree};

use crate::graphql::{Issue, PullRequest};
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

    pub(crate) fn insert_issues(&self, resp: Vec<Issue>, owner: &str, name: &str) -> Result<()> {
        for item in resp {
            let key: String = format!("{owner}/{name}#{}", item.number);
            Database::insert(&key, item, &self.issue_tree)?;
        }
        Ok(())
    }

    pub(crate) fn insert_pull_requests(
        &self,
        resp: Vec<PullRequest>,
        owner: &str,
        name: &str,
    ) -> Result<()> {
        for item in resp {
            let key: String = format!("{owner}/{name}#{}", item.number);
            Database::insert(&key, item, &self.pull_request_tree)?;
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

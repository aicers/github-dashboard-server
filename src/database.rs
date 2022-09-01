use crate::{
    github::{GitHubIssue, GitHubPullRequests},
    graphql::{Issue, PagingType, PullRequest},
};
use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::Serialize;
use sled::{Db, IVec, Tree};

const ISSUE_TREE_NAME: &str = "issues";
const PULL_REQUEST_TREE_NAME: &str = "pull_requests";

#[derive(Clone)]
pub struct Database {
    db: Db,
    issue_tree: Tree,
    pull_request_tree: Tree,
}

impl Database {
    fn connect_db(path: &str) -> Result<Db> {
        Ok(sled::open(path)?)
    }

    fn connect_trees(db: &Db) -> Result<(Tree, Tree)> {
        let issue_tree = db.open_tree(ISSUE_TREE_NAME)?;
        let pull_request_tree = db.open_tree(PULL_REQUEST_TREE_NAME)?;
        Ok((issue_tree, pull_request_tree))
    }

    pub fn connect(db_path: &str) -> Result<Database> {
        let db = Database::connect_db(db_path)?;
        let (issue_tree, pull_request_tree) = Database::connect_trees(&db)?;
        Ok(Database {
            db,
            issue_tree,
            pull_request_tree,
        })
    }

    /// Returns the data store for issues.
    pub fn issue_store(&self) -> &Tree {
        &self.issue_tree
    }

    /// Returns the data store for pull request.
    pub fn pull_request_store(&self) -> &Tree {
        &self.pull_request_tree
    }

    fn insert<T: Serialize>(key: &str, val: T, tree: &Tree) -> Result<()> {
        tree.insert(key, bincode::serialize(&val)?)?;
        Ok(())
    }

    #[allow(unused)]
    pub fn select(key: &str, tree: &Tree) -> Result<String> {
        if let Ok(Some(val)) = tree.get(key) {
            let result: String = bincode::deserialize(&val)?;
            return Ok(result);
        }
        bail!("Failed to get tree value");
    }

    pub fn insert_db<T: Serialize>(&self, key: &str, val: T) -> Result<()> {
        self.db.insert(key, bincode::serialize(&val)?)?;
        Ok(())
    }

    pub fn select_db(&self, key: &str) -> Result<String> {
        if let Ok(Some(val)) = self.db.get(key) {
            let result: String = bincode::deserialize(&val)?;
            return Ok(result);
        }
        bail!("Failed to get db value");
    }

    pub fn delete_db(&self, key: &str) -> Result<String> {
        if let Ok(Some(val)) = self.db.remove(key) {
            let result: String = bincode::deserialize(&val)?;
            return Ok(result);
        }
        bail!("Failed to remove tree value");
    }

    #[allow(unused)]
    pub fn delete(key: &str, tree: &Tree) -> Result<String> {
        if let Ok(Some(val)) = tree.remove(key) {
            let result: String = bincode::deserialize(&val)?;
            return Ok(result);
        }
        bail!("Failed to remove tree value");
    }

    #[allow(unused)]
    pub fn delete_all(tree: &Tree) -> Result<()> {
        tree.clear()?;
        Ok(())
    }

    pub fn insert_issues(&self, resp: Vec<GitHubIssue>, owner: &str, name: &str) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{}/{}#{}", owner, name, item.number);
            Database::insert(&keystr, (&item.title, &item.author), &self.issue_tree)?;
        }
        Ok(())
    }

    pub fn insert_pull_requests(
        &self,
        resp: Vec<GitHubPullRequests>,
        owner: &str,
        name: &str,
    ) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{}/{}#{}", owner, name, item.number);
            Database::insert(
                &keystr,
                (&item.title, &item.assignees, &item.reviewers),
                &self.pull_request_tree,
            )?;
        }
        Ok(())
    }

    pub fn has_prev(key: String, tree: &Tree) -> Result<bool> {
        Ok(tree.get_lt(key)?.is_some())
    }

    pub fn has_next(key: String, tree: &Tree) -> Result<bool> {
        Ok(tree.get_gt(key)?.is_some())
    }

    pub fn select_issue_range(&self, p_type: PagingType) -> Result<Vec<Issue>> {
        let tree = &self.issue_tree;
        let mut range_list: Vec<(IVec, IVec)>;
        match p_type {
            PagingType::All => {
                range_list = tree
                    .iter()
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::First(f_val) => {
                range_list = tree
                    .iter()
                    .take(f_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::Last(l_val) => {
                let len = tree.len();
                let skip = if len > l_val { len - l_val } else { 0 };
                range_list = tree
                    .iter()
                    .skip(skip)
                    .take(l_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::AfterFirst(cursor, f_val) => {
                range_list = tree
                    .range(cursor..)
                    .skip(1)
                    .take(f_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::BeforeLast(cursor, l_val) => {
                range_list = tree
                    .range(..cursor)
                    .rev()
                    .take(l_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
                range_list.reverse();
            }
        }
        get_issue_list(range_list)
    }

    pub fn select_pull_request_range(&self, p_type: PagingType) -> Result<Vec<PullRequest>> {
        let tree = &self.pull_request_tree;
        let mut range_list: Vec<(IVec, IVec)>;
        match p_type {
            PagingType::All => {
                range_list = tree
                    .iter()
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::First(f_val) => {
                range_list = tree
                    .iter()
                    .take(f_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::Last(l_val) => {
                let len = tree.len();
                let skip = if len > l_val { len - l_val } else { 0 };
                range_list = tree
                    .iter()
                    .skip(skip)
                    .take(l_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::AfterFirst(cursor, f_val) => {
                range_list = tree
                    .range(cursor..)
                    .skip(1)
                    .take(f_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::BeforeLast(cursor, l_val) => {
                range_list = tree
                    .range(..cursor)
                    .rev()
                    .take(l_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
                range_list.reverse();
            }
        }
        get_pull_request_list(range_list)
    }
}

fn get_issue_list(range_list: Vec<(IVec, IVec)>) -> Result<Vec<Issue>> {
    let mut issue_list = Vec::new();

    for (key, val) in range_list {
        let (owner, repo, number) = parse_key(&key)?;
        let (title, author) = bincode::deserialize::<(String, String)>(&val)
            .unwrap_or(("No title".to_string(), "No author".to_string()));
        issue_list.push(Issue {
            owner,
            repo,
            number: i32::try_from(number).unwrap_or(i32::MAX),
            title,
            author,
        });
    }
    Ok(issue_list)
}

fn get_pull_request_list(range_list: Vec<(IVec, IVec)>) -> Result<Vec<PullRequest>> {
    let mut pull_request_list = Vec::new();

    for (key, val) in range_list {
        let (owner, repo, number) = parse_key(&key)?;
        let (title, assignees, reviewers) =
            bincode::deserialize::<(String, Vec<String>, Vec<String>)>(&val).unwrap();
        pull_request_list.push(PullRequest {
            owner,
            repo,
            number: i32::try_from(number).unwrap_or(i32::MAX),
            title,
            assignees,
            reviewers,
        });
    }
    Ok(pull_request_list)
}

fn parse_key(key: &[u8]) -> Result<(String, String, i64)> {
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

use crate::{
    github::{GitHubIssue, GitHubPRs},
    graphql::{Issue, PagingType, PullRequest},
    ISSUE_TREE_NAME, PR_TREE_NAME,
};
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use sled::{Db, IVec, Tree};

#[derive(Clone)]
pub struct Database {
    #[allow(unused)]
    db: Db,
    issue_tree: Tree,
    pr_tree: Tree,
}

impl Database {
    fn connect_db(path: &str) -> Result<Db> {
        Ok(sled::open(path)?)
    }

    fn connect_trees(db: &Db, trees: &[&str]) -> Result<(Tree, Tree)> {
        let issue_tree = db.open_tree(trees[0])?;
        let pr_tree = db.open_tree(trees[1])?;
        Ok((issue_tree, pr_tree))
    }

    pub fn connect(db_path: &str, trees: &[&str]) -> Result<Database> {
        let db = Database::connect_db(db_path)?;
        let (issue_tree, pr_tree) = Database::connect_trees(&db, trees)?;
        Ok(Database {
            db,
            issue_tree,
            pr_tree,
        })
    }

    fn insert(key: &str, val: &str, tree: &Tree) -> Result<()> {
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
            Database::insert(&keystr, &item.title, &self.issue_tree)?;
        }
        Ok(())
    }

    pub fn insert_prs(&self, resp: Vec<GitHubPRs>, owner: &str, name: &str) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{}/{}#{}", owner, name, item.number);
            Database::insert(&keystr, &item.title, &self.pr_tree)?;
        }
        Ok(())
    }

    pub fn get_prev_exist(key: String, tree: &Tree) -> Result<bool> {
        if tree.get_lt(key)?.is_some() {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get_next_exist(key: String, tree: &Tree) -> Result<bool> {
        if tree.get_gt(key)?.is_some() {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn tree(&self, t_name: &str) -> Result<&Tree> {
        match t_name {
            ISSUE_TREE_NAME => Ok(&self.issue_tree),
            PR_TREE_NAME => Ok(&self.pr_tree),
            _ => Err(anyhow!("Invalid tree name")),
        }
    }

    pub fn select_issue_range(p_type: PagingType, tree: &Tree) -> Result<Vec<Issue>> {
        let mut range_list: Vec<(IVec, IVec)>;
        let re = Regex::new(r"(?P<owner>\S+)/(?P<name>\S+)#(?P<number>[0-9]+)")?;
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
        get_issue_list(&re, range_list)
    }

    pub fn select_pr_range(p_type: PagingType, tree: &Tree) -> Result<Vec<PullRequest>> {
        let mut range_list: Vec<(IVec, IVec)>;
        let re = Regex::new(r"(?P<owner>\S+)/(?P<name>\S+)#(?P<number>[0-9]+)")?;
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
        get_pr_list(&re, range_list)
    }
}

fn get_issue_list(re: &Regex, range_list: Vec<(IVec, IVec)>) -> Result<Vec<Issue>> {
    let mut issue_list = Vec::new();

    for (key, val) in range_list {
        match re.captures(String::from_utf8(key.to_vec())?.as_str()) {
            Some(caps) => issue_list.push(Issue {
                owner: match caps.name("owner") {
                    Some(x) => String::from(x.as_str()),
                    None => unreachable!(),
                },
                repo: match caps.name("name") {
                    Some(x) => String::from(x.as_str()),
                    None => unreachable!(),
                },
                number: match caps.name("number") {
                    Some(x) => x.as_str().trim().parse::<i32>()?,
                    None => unreachable!(),
                },
                title: bincode::deserialize::<String>(&val)?,
            }),
            None => eprintln!("key doesn't match owner/name#number"),
        }
    }
    Ok(issue_list)
}

fn get_pr_list(re: &Regex, range_list: Vec<(IVec, IVec)>) -> Result<Vec<PullRequest>> {
    let mut pr_list = Vec::new();

    for (key, val) in range_list {
        match re.captures(String::from_utf8(key.to_vec())?.as_str()) {
            Some(caps) => pr_list.push(PullRequest {
                owner: match caps.name("owner") {
                    Some(x) => String::from(x.as_str()),
                    None => unreachable!(),
                },
                repo: match caps.name("name") {
                    Some(x) => String::from(x.as_str()),
                    None => unreachable!(),
                },
                number: match caps.name("number") {
                    Some(x) => x.as_str().trim().parse::<i32>()?,
                    None => unreachable!(),
                },
                title: bincode::deserialize::<String>(&val)?,
            }),
            None => eprintln!("key doesn't match owner/name#number"),
        }
    }
    Ok(pr_list)
}

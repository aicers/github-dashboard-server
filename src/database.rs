use crate::{
    github::GitHubIssue,
    graphql::{Issue, PagingType},
};
use anyhow::{bail, Result};
use regex::Regex;
use sled::{Db, IVec, Tree};

#[derive(Clone)]
pub struct Database {
    #[allow(unused)]
    db: Db,
    tree: Tree,
}

impl Database {
    fn connect_db(path: &str) -> Result<Db> {
        Ok(sled::open(path)?)
    }

    fn connect_tree(db: &Db, t_name: &str) -> Result<Tree> {
        Ok(db.open_tree(bincode::serialize(t_name)?)?)
    }

    pub fn connect(db_path: &str, t_name: &str) -> Result<Database> {
        let db = Database::connect_db(db_path)?;
        let tree = Database::connect_tree(&db, t_name)?;
        Ok(Database { db, tree })
    }

    pub fn insert(&self, key: &str, val: &str) -> Result<()> {
        self.tree.insert(key, bincode::serialize(&val)?)?;
        Ok(())
    }

    #[allow(unused)]
    pub fn select(&self, key: &str) -> Result<String> {
        if let Ok(Some(val)) = self.tree.get(key) {
            let result: String = bincode::deserialize(&val)?;
            return Ok(result);
        }
        bail!("Failed to get tree value");
    }

    #[allow(unused)]
    pub fn delete(&self, key: &str) -> Result<String> {
        if let Ok(Some(val)) = self.tree.remove(key) {
            let result: String = bincode::deserialize(&val)?;
            return Ok(result);
        }
        bail!("Failed to remove tree value");
    }

    #[allow(unused)]
    pub fn delete_all(&self) -> Result<()> {
        self.tree.clear()?;
        Ok(())
    }

    pub fn insert_issues(&self, resp: Vec<GitHubIssue>, owner: &str, name: &str) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{}/{}#{}", owner, name, item.number);
            self.insert(&keystr, &item.title)?;
        }
        Ok(())
    }

    pub fn get_prev_exist(&self, key: String) -> Result<bool> {
        if self.tree.get_lt(key)?.is_some() {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get_next_exist(&self, key: String) -> Result<bool> {
        if self.tree.get_gt(key)?.is_some() {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn select_range(&self, p_type: PagingType) -> Result<Vec<Issue>> {
        let mut range_list: Vec<(IVec, IVec)>;
        let re = Regex::new(r"(?P<owner>\S+)/(?P<name>\S+)#(?P<number>[0-9]+)")?;
        match p_type {
            PagingType::All => {
                range_list = self
                    .tree
                    .iter()
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::First(f_val) => {
                range_list = self
                    .tree
                    .iter()
                    .take(f_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::Last(l_val) => {
                let len = self.tree.len();
                let skip = if len > l_val { len - l_val } else { 0 };
                range_list = self
                    .tree
                    .iter()
                    .skip(skip)
                    .take(l_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::AfterFirst(cursor, f_val) => {
                range_list = self
                    .tree
                    .range(cursor..)
                    .skip(1)
                    .take(f_val)
                    .filter_map(std::result::Result::ok)
                    .collect::<Vec<(IVec, IVec)>>();
            }
            PagingType::BeforeLast(cursor, l_val) => {
                range_list = self
                    .tree
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
}

fn get_issue_list(re: &Regex, range_list: Vec<(IVec, IVec)>) -> Result<Vec<Issue>> {
    let mut issue_list: Vec<Issue> = Vec::new();
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

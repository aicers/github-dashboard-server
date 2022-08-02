use crate::{github::GitHubIssue, graphql::Issue};
use anyhow::{bail, Result};
use regex::Regex;
use sled::{Db, Tree};

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

    pub fn select_all(&self) -> Result<Vec<Issue>> {
        let mut all_vec: Vec<Issue> = Vec::new();
        let re = Regex::new(r"(?P<owner>\S+)/(?P<name>\S+)#(?P<number>[0-9]+)")?;
        for (key, val) in self.tree.iter().filter_map(std::result::Result::ok) {
            match re.captures(String::from_utf8(key.to_vec())?.as_str()) {
                Some(caps) => all_vec.push(Issue {
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
        Ok(all_vec)
    }

    pub fn insert_issues(&self, resp: Vec<GitHubIssue>, owner: &str, name: &str) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{}/{}#{}", owner, name, item.number);
            self.insert(&keystr, &item.title)?;
        }
        Ok(())
    }
}

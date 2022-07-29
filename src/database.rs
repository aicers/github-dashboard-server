use crate::github::GitHubIssue;
use anyhow::{bail, Result};
use sled::{Db, Tree};

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

    pub fn insert(&self, key: i32, val: &str) -> Result<()> {
        self.tree
            .insert(&bincode::serialize(&key)?, bincode::serialize(&val)?)?;
        Ok(())
    }

    #[allow(unused)]
    pub fn select(&self, key: i32) -> Result<String> {
        if let Ok(Some(val)) = self.tree.get(bincode::serialize(&key)?) {
            let result: String = bincode::deserialize(&val)?;
            return Ok(result);
        }
        bail!("Failed to get tree value");
    }

    #[allow(unused)]
    pub fn delete(&self, key: i32) -> Result<String> {
        if let Ok(Some(val)) = self.tree.remove(bincode::serialize(&key)?) {
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

    #[allow(unused)]
    pub fn select_all(&self) -> Result<Vec<String>> {
        let mut all_vec: Vec<String> = Vec::new();
        for (_, val) in self.tree.iter().filter_map(std::result::Result::ok) {
            all_vec.push(bincode::deserialize::<String>(&val)?);
        }
        Ok(all_vec)
    }

    pub fn insert_issues(&self, resp: Vec<GitHubIssue>) -> Result<()> {
        for item in resp {
            self.insert(item.number, item.title.as_str())?;
        }
        Ok(())
    }
}

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use fjall::{Keyspace, PartitionHandle};
use regex::Regex;
use serde::Serialize;

pub mod discussion;
pub mod issue;

pub(crate) use discussion::DiscussionDbSchema;
pub(crate) use issue::GitHubIssue;

use crate::{api::pull_request::PullRequest, outbound::GitHubPullRequestNode};

const GLOBAL_PARTITION_NAME: &str = "global";
const ISSUE_PARTITION_NAME: &str = "issues";
const PULL_REQUEST_PARTITION_NAME: &str = "pull_requests";
const DISCUSSION_PARTITION_NAME: &str = "discussions";

#[derive(Clone)]
pub(crate) struct Database {
    keyspace: Keyspace,
    issue_partition: PartitionHandle,
    pull_request_partition: PartitionHandle,
    discussion_partition: PartitionHandle,
}

impl Database {
    fn connect_keyspace(path: &Path) -> Result<Keyspace> {
        Ok(fjall::Config::new(path).open()?)
    }

    fn connect_partitions(
        keyspace: &Keyspace,
    ) -> Result<(PartitionHandle, PartitionHandle, PartitionHandle)> {
        let options = fjall::PartitionCreateOptions::default();
        let issue_partition = keyspace.open_partition(ISSUE_PARTITION_NAME, options.clone())?;
        let pull_request_partition =
            keyspace.open_partition(PULL_REQUEST_PARTITION_NAME, options.clone())?;
        let discussion_partition =
            keyspace.open_partition(DISCUSSION_PARTITION_NAME, options.clone())?;
        Ok((
            issue_partition,
            pull_request_partition,
            discussion_partition,
        ))
    }

    pub(crate) fn connect(db_path: &Path) -> Result<Database> {
        let keyspace = Database::connect_keyspace(db_path)?;
        let (issue_partition, pull_request_partition, discussion_partition) =
            Database::connect_partitions(&keyspace)?;
        Ok(Database {
            keyspace,
            issue_partition,
            pull_request_partition,
            discussion_partition,
        })
    }

    fn insert<T: Serialize>(key: &str, val: T, partition: &PartitionHandle) -> Result<()> {
        partition.insert(key, bincode::serialize(&val)?)?;
        Ok(())
    }

    pub(crate) fn insert_db<T: Serialize>(&self, key: &str, val: T) -> Result<()> {
        let global_partition = self.keyspace.open_partition(
            GLOBAL_PARTITION_NAME,
            fjall::PartitionCreateOptions::default(),
        )?;
        global_partition.insert(key, bincode::serialize(&val)?)?;
        Ok(())
    }

    pub(crate) fn select_db(&self, key: &str) -> Result<String> {
        let global_partition = self.keyspace.open_partition(
            GLOBAL_PARTITION_NAME,
            fjall::PartitionCreateOptions::default(),
        )?;
        if let Some(val) = global_partition.get(key)? {
            let result: String = bincode::deserialize(&val)?;
            return Ok(result);
        }
        bail!("Failed to get db value");
    }

    pub(crate) fn insert_pull_requests(
        &self,
        resp: Vec<GitHubPullRequestNode>,
        owner: &str,
        name: &str,
    ) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{owner}/{name}#{}", item.number);
            Database::insert(&keystr, item, &self.pull_request_partition)?;
        }
        Ok(())
    }

    pub(crate) fn pull_requests(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Iter<PullRequest> {
        let start = start.unwrap_or(b"\x00");
        if let Some(end) = end {
            Iter::new(self.pull_request_partition.range(start..end))
        } else {
            Iter::new(self.pull_request_partition.range(start..))
        }
    }
}

pub(crate) trait TryFromKeyValue {
    fn try_from_key_value(key: &[u8], value: &[u8]) -> Result<Self>
    where
        Self: Sized;
}

pub(crate) struct Iter<T> {
    inner: Box<dyn DoubleEndedIterator<Item = fjall::Result<(fjall::Slice, fjall::Slice)>>>,
    phantom: std::marker::PhantomData<T>,
}

impl<T> Iter<T> {
    fn new(
        iter: impl DoubleEndedIterator<Item = fjall::Result<(fjall::Slice, fjall::Slice)>> + 'static,
    ) -> Self
    where
        T: TryFromKeyValue,
    {
        Self {
            inner: Box::new(iter),
            phantom: std::marker::PhantomData,
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

pub(crate) fn parse_key(key: &[u8]) -> Result<(String, String, i32)> {
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
            .parse::<i32>()
            .context("invalid key")?;
        Ok((owner, name, number))
    } else {
        Err(anyhow!("invalid key"))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::wildcard_imports)]
    use anyhow::Result;

    use super::*;

    // Mock implementation for testing
    #[derive(Debug, PartialEq)]
    struct TestItem {
        key: String,
        value: i32,
    }

    impl TryFromKeyValue for TestItem {
        fn try_from_key_value(key: &[u8], value: &[u8]) -> Result<Self> {
            let key_str = String::from_utf8(key.to_vec())?;
            let value_i32 = i32::from_le_bytes(
                value
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid value format"))?,
            );
            Ok(TestItem {
                key: key_str,
                value: value_i32,
            })
        }
    }

    fn create_mock_fjall_iter(
        items: Vec<(String, i32)>,
    ) -> impl DoubleEndedIterator<Item = fjall::Result<(fjall::Slice, fjall::Slice)>> {
        items.into_iter().map(|(key, value)| {
            let key_bytes = key.into_bytes();
            let value_bytes = value.to_le_bytes().to_vec();
            Ok((
                fjall::Slice::from(key_bytes),
                fjall::Slice::from(value_bytes),
            ))
        })
    }

    #[test]
    fn parse_key() {
        let key = "rust-lang/rust#12345";
        let (owner, name, number) = super::parse_key(key.as_bytes()).unwrap();
        assert_eq!(owner, "rust-lang");
        assert_eq!(name, "rust");
        assert_eq!(number, 12345);
    }

    #[test]
    fn iter_forward_empty() {
        let mock_iter = create_mock_fjall_iter(vec![]);
        let mut iter: Iter<TestItem> = Iter::new(mock_iter);

        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_forward_single_item() {
        let mock_iter = create_mock_fjall_iter(vec![("key1".to_string(), 42)]);
        let mut iter: Iter<TestItem> = Iter::new(mock_iter);

        let item = iter.next().unwrap().unwrap();
        assert_eq!(item.key, "key1");
        assert_eq!(item.value, 42);

        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_forward_multiple_items() {
        let mock_iter = create_mock_fjall_iter(vec![
            ("key1".to_string(), 10),
            ("key2".to_string(), 20),
            ("key3".to_string(), 30),
        ]);
        let mut iter: Iter<TestItem> = Iter::new(mock_iter);

        let item1 = iter.next().unwrap().unwrap();
        assert_eq!(item1.key, "key1");
        assert_eq!(item1.value, 10);

        let item2 = iter.next().unwrap().unwrap();
        assert_eq!(item2.key, "key2");
        assert_eq!(item2.value, 20);

        let item3 = iter.next().unwrap().unwrap();
        assert_eq!(item3.key, "key3");
        assert_eq!(item3.value, 30);

        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_reverse_empty() {
        let mock_iter = create_mock_fjall_iter(vec![]);
        let mut iter: Iter<TestItem> = Iter::new(mock_iter);

        assert!(iter.next_back().is_none());
    }

    #[test]
    fn iter_reverse_single_item() {
        let mock_iter = create_mock_fjall_iter(vec![("key1".to_string(), 42)]);
        let mut iter: Iter<TestItem> = Iter::new(mock_iter);

        let item = iter.next_back().unwrap().unwrap();
        assert_eq!(item.key, "key1");
        assert_eq!(item.value, 42);

        assert!(iter.next_back().is_none());
    }

    #[test]
    fn iter_reverse_multiple_items() {
        let mock_iter = create_mock_fjall_iter(vec![
            ("key1".to_string(), 10),
            ("key2".to_string(), 20),
            ("key3".to_string(), 30),
        ]);
        let mut iter: Iter<TestItem> = Iter::new(mock_iter);

        // Should get items in reverse order
        let item3 = iter.next_back().unwrap().unwrap();
        assert_eq!(item3.key, "key3");
        assert_eq!(item3.value, 30);

        let item2 = iter.next_back().unwrap().unwrap();
        assert_eq!(item2.key, "key2");
        assert_eq!(item2.value, 20);

        let item1 = iter.next_back().unwrap().unwrap();
        assert_eq!(item1.key, "key1");
        assert_eq!(item1.value, 10);

        assert!(iter.next_back().is_none());
    }

    #[test]
    fn iter_mixed_forward_and_reverse() {
        let mock_iter = create_mock_fjall_iter(vec![
            ("key1".to_string(), 10),
            ("key2".to_string(), 20),
            ("key3".to_string(), 30),
            ("key4".to_string(), 40),
        ]);
        let mut iter: Iter<TestItem> = Iter::new(mock_iter);

        // Start with forward iteration
        let item1 = iter.next().unwrap().unwrap();
        assert_eq!(item1.key, "key1");
        assert_eq!(item1.value, 10);

        // Switch to reverse - this should convert the iterator and return the last item
        let item4 = iter.next_back().unwrap().unwrap();
        assert_eq!(item4.key, "key4");
        assert_eq!(item4.value, 40);

        // Continue with reverse
        let item3 = iter.next_back().unwrap().unwrap();
        assert_eq!(item3.key, "key3");
        assert_eq!(item3.value, 30);

        let item2 = iter.next_back().unwrap().unwrap();
        assert_eq!(item2.key, "key2");
        assert_eq!(item2.value, 20);

        // All items consumed
        assert!(iter.next_back().is_none());
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_reverse_then_forward() {
        let mock_iter = create_mock_fjall_iter(vec![
            ("key1".to_string(), 10),
            ("key2".to_string(), 20),
            ("key3".to_string(), 30),
        ]);
        let mut iter: Iter<TestItem> = Iter::new(mock_iter);

        // Start with reverse iteration
        let item3 = iter.next_back().unwrap().unwrap();
        assert_eq!(item3.key, "key3");
        assert_eq!(item3.value, 30);

        // Switch to forward iteration (should start from the first item)
        let item1 = iter.next().unwrap().unwrap();
        assert_eq!(item1.key, "key1");
        assert_eq!(item1.value, 10);

        let item2 = iter.next().unwrap().unwrap();
        assert_eq!(item2.key, "key2");
        assert_eq!(item2.value, 20);

        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_error_handling() {
        // Create an iterator that will produce an error
        let error_iter =
            std::iter::once(Err(fjall::Error::Io(std::io::Error::other("test error"))));
        let mut iter: Iter<TestItem> = Iter::new(error_iter);

        let result = iter.next().unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test error"));
    }

    #[test]
    fn iter_collect_forward() {
        let mock_iter = create_mock_fjall_iter(vec![
            ("key1".to_string(), 10),
            ("key2".to_string(), 20),
            ("key3".to_string(), 30),
        ]);
        let iter: Iter<TestItem> = Iter::new(mock_iter);

        let items: Vec<_> = iter.collect();
        assert_eq!(items.len(), 3);

        let item1 = items[0].as_ref().unwrap();
        assert_eq!(item1.key, "key1");
        assert_eq!(item1.value, 10);

        let item2 = items[1].as_ref().unwrap();
        assert_eq!(item2.key, "key2");
        assert_eq!(item2.value, 20);

        let item3 = items[2].as_ref().unwrap();
        assert_eq!(item3.key, "key3");
        assert_eq!(item3.value, 30);
    }

    #[test]
    fn iter_collect_reverse() {
        let mock_iter = create_mock_fjall_iter(vec![
            ("key1".to_string(), 10),
            ("key2".to_string(), 20),
            ("key3".to_string(), 30),
        ]);
        let iter: Iter<TestItem> = Iter::new(mock_iter);

        let items: Vec<_> = iter.rev().collect();
        assert_eq!(items.len(), 3);

        let item3 = items[0].as_ref().unwrap();
        assert_eq!(item3.key, "key3");
        assert_eq!(item3.value, 30);

        let item2 = items[1].as_ref().unwrap();
        assert_eq!(item2.key, "key2");
        assert_eq!(item2.value, 20);

        let item1 = items[2].as_ref().unwrap();
        assert_eq!(item1.key, "key1");
        assert_eq!(item1.value, 10);
    }
}

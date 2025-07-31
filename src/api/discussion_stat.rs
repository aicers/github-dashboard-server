use async_graphql::{Context, InputObject, Object, Result, SimpleObject};

use crate::{
    api::{DateTimeUtc, Discussion},
    database::Iter,
    Database,
};

#[derive(InputObject, Debug)]
pub(crate) struct DiscussionStatFilter {
    /// Filter by discussion author.
    author: Option<String>,
    /// Filter by repository name.
    repo: Option<String>,
    /// Start of the creation datetime range. (inclusive)
    /// Example format: "yyyy-MM-ddTHH:mm:ssZ"
    begin: Option<DateTimeUtc>,
    /// End of the creation datetime range. (exclusive)
    /// Example format: "yyyy-MM-ddTHH:mm:ssZ"
    end: Option<DateTimeUtc>,
}

impl DiscussionStatFilter {
    fn filter_discussions(&self, discussions: Iter<Discussion>) -> Vec<Discussion> {
        discussions
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|d| {
                self.author
                    .as_ref()
                    .is_none_or(|author| d.author == *author)
                    && self.repo.as_ref().is_none_or(|repo| d.repo == *repo)
                    && self
                        .begin
                        .as_ref()
                        .is_none_or(|begin| d.created_at >= *begin)
                    && self.end.as_ref().is_none_or(|end| d.created_at < *end)
            })
            .collect()
    }
}

#[derive(Default)]
pub(super) struct DiscussionStatQuery {}

#[derive(SimpleObject)]
struct DiscussionStat {
    /// The total number of discussions.
    total_count: i32,
}

#[Object]
impl DiscussionStatQuery {
    #[allow(clippy::unused_async)]
    async fn discussion_stat(
        &self,
        ctx: &Context<'_>,
        filter: DiscussionStatFilter,
    ) -> Result<DiscussionStat> {
        let db = ctx.data::<Database>()?;
        let discussions = db.discussions(None, None);
        let filtered = filter.filter_discussions(discussions);
        let total_count = filtered.len().try_into()?;

        Ok(DiscussionStat { total_count })
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use crate::{api::TestSchema, database::DiscussionDbSchema};

    fn create_discussions(n: usize) -> Vec<DiscussionDbSchema> {
        (0..n)
            .map(|i| DiscussionDbSchema {
                number: i.try_into().unwrap(),
                ..Default::default()
            })
            .collect()
    }

    fn parse(date: &str) -> Timestamp {
        date.parse().unwrap()
    }

    #[tokio::test]
    async fn total_count_by_author() {
        let schema = TestSchema::new().await;
        let mut discussions = create_discussions(3);
        discussions[0].author = "foo".to_string();
        schema
            .db
            .insert_discussions(discussions, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            discussionStat(filter: {author: "foo"}) {
                totalCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["discussionStat"]["totalCount"], 1);
    }

    #[tokio::test]
    async fn total_count_by_begin_end() {
        let schema = TestSchema::new().await;
        let mut discussions = create_discussions(3);
        discussions[1].created_at = parse("2025-01-05T00:00:00Z");
        discussions[2].created_at = parse("2025-01-06T00:00:00Z");

        schema
            .db
            .insert_discussions(discussions, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            discussionStat(filter: {begin: "2025-01-06T00:00:00Z"}) {
                totalCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["discussionStat"]["totalCount"], 1);

        let query = r#"
        {
            discussionStat(filter: {begin: "2025-01-05T00:00:00Z", end: "2025-01-06T00:00:00Z"}) {
                totalCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();

        assert_eq!(data["discussionStat"]["totalCount"], 1);
    }

    #[tokio::test]
    async fn total_count_by_repo() {
        let schema = TestSchema::new().await;
        let server_discussions = create_discussions(2);
        let client_discussions = create_discussions(1);

        schema
            .db
            .insert_discussions(server_discussions, "aicers", "github-dashboard-server")
            .unwrap();
        schema
            .db
            .insert_discussions(client_discussions, "aicers", "github-dashboard-client")
            .unwrap();
        let query = r#"
        {
            discussionStat(filter: {repo: "github-dashboard-client"}) {
                totalCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();

        assert_eq!(data["discussionStat"]["totalCount"], 1);
    }
}

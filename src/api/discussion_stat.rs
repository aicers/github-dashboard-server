use async_graphql::{Context, InputObject, Object, Result, SimpleObject};

use crate::{
    api::{discussion::DiscussionComment, DateTimeUtc, Discussion},
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

    fn filter_comments(&self, discussions: Iter<Discussion>) -> Vec<DiscussionComment> {
        discussions
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|d| self.repo.as_ref().is_none_or(|repo| d.repo == *repo))
            .flat_map(|d| d.comments)
            .filter(|c| {
                self.author
                    .as_ref()
                    .is_none_or(|author| c.author == *author)
                    && self
                        .begin
                        .as_ref()
                        .is_none_or(|begin| c.created_at >= *begin)
                    && self.end.as_ref().is_none_or(|end| c.created_at < *end)
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
    /// The total number of comments across all discussions.
    comment_count: i32,
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

        let total_count = filter
            .filter_discussions(db.discussions(None, None))
            .len()
            .try_into()?;

        let comment_count = filter
            .filter_comments(db.discussions(None, None))
            .len()
            .try_into()?;

        Ok(DiscussionStat {
            total_count,
            comment_count,
        })
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

    fn create_comment(
        body: &str,
        author: &str,
        created_at: &str,
    ) -> crate::database::discussion::Comment {
        crate::database::discussion::Comment {
            body: body.to_string(),
            author: author.to_string(),
            created_at: parse(created_at),
            updated_at: parse(created_at),
            published_at: Some(parse(created_at)),
            url: format!(
                "https://example.com/{}",
                body.replace(' ', "_").to_lowercase()
            ),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn total_count_by_author() {
        let schema = TestSchema::new();
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
        let schema = TestSchema::new();
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
        let schema = TestSchema::new();
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

    #[tokio::test]
    async fn comment_count_basic() {
        let schema = TestSchema::new();
        let mut discussions = create_discussions(2);

        // Add comments to first discussion
        discussions[0].comments.nodes = vec![
            create_comment("First comment", "alice", "2025-01-01T00:00:00Z"),
            create_comment("Second comment", "bob", "2025-01-02T00:00:00Z"),
        ];
        discussions[0].comments.total_count = 2;

        // Add one comment to second discussion
        discussions[1].comments.nodes = vec![create_comment(
            "Third comment",
            "charlie",
            "2025-01-03T00:00:00Z",
        )];
        discussions[1].comments.total_count = 1;

        schema
            .db
            .insert_discussions(discussions, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r"
        {
            discussionStat(filter: {}) {
                commentCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["discussionStat"]["commentCount"], 3);
    }

    #[tokio::test]
    async fn comment_count_by_author() {
        let schema = TestSchema::new();
        let mut discussions = create_discussions(2);

        // Add comments with different authors
        discussions[0].comments.nodes = vec![
            create_comment("Comment by alice", "alice", "2025-01-01T00:00:00Z"),
            create_comment("Comment by bob", "bob", "2025-01-02T00:00:00Z"),
        ];
        discussions[0].comments.total_count = 2;

        discussions[1].comments.nodes = vec![create_comment(
            "Another comment by alice",
            "alice",
            "2025-01-03T00:00:00Z",
        )];
        discussions[1].comments.total_count = 1;

        schema
            .db
            .insert_discussions(discussions, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r#"
        {
            discussionStat(filter: {author: "alice"}) {
                commentCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["discussionStat"]["commentCount"], 2);
    }

    #[tokio::test]
    async fn comment_count_by_date_range() {
        let schema = TestSchema::new();
        let mut discussions = create_discussions(2);

        // Add comments with different dates
        discussions[0].comments.nodes = vec![
            create_comment("Early comment", "alice", "2025-01-01T00:00:00Z"),
            create_comment("Late comment", "bob", "2025-01-10T00:00:00Z"),
        ];
        discussions[0].comments.total_count = 2;

        discussions[1].comments.nodes = vec![create_comment(
            "Middle comment",
            "charlie",
            "2025-01-05T00:00:00Z",
        )];
        discussions[1].comments.total_count = 1;

        schema
            .db
            .insert_discussions(discussions, "aicers", "github-dashboard-server")
            .unwrap();

        // Test filtering by begin date
        let query = r#"
        {
            discussionStat(filter: {begin: "2025-01-05T00:00:00Z"}) {
                commentCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["discussionStat"]["commentCount"], 2);

        // Test filtering by date range
        let query = r#"
        {
            discussionStat(filter: {begin: "2025-01-02T00:00:00Z", end: "2025-01-08T00:00:00Z"}) {
                commentCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["discussionStat"]["commentCount"], 1);
    }

    #[tokio::test]
    async fn comment_count_by_repo() {
        let schema = TestSchema::new();
        let mut server_discussions = create_discussions(1);
        let mut client_discussions = create_discussions(1);

        // Add comments to server discussion
        server_discussions[0].comments.nodes = vec![create_comment(
            "Server comment",
            "alice",
            "2025-01-01T00:00:00Z",
        )];
        server_discussions[0].comments.total_count = 1;

        // Add comments to client discussion
        client_discussions[0].comments.nodes = vec![
            create_comment("Client comment 1", "bob", "2025-01-02T00:00:00Z"),
            create_comment("Client comment 2", "charlie", "2025-01-03T00:00:00Z"),
        ];
        client_discussions[0].comments.total_count = 2;

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
                commentCount
            }
        }"#;
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["discussionStat"]["commentCount"], 2);
    }

    #[tokio::test]
    async fn comment_count_empty_discussions() {
        let schema = TestSchema::new();
        let discussions = create_discussions(3); // All discussions have no comments by default

        schema
            .db
            .insert_discussions(discussions, "aicers", "github-dashboard-server")
            .unwrap();

        let query = r"
        {
            discussionStat(filter: {}) {
                commentCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["discussionStat"]["commentCount"], 0);
    }

    #[tokio::test]
    async fn comment_count_mixed_scenarios() {
        let schema = TestSchema::new();
        let mut discussions = create_discussions(3);

        // First discussion: 2 comments
        discussions[0].comments.nodes = vec![
            create_comment("Comment 1", "alice", "2025-01-01T00:00:00Z"),
            create_comment("Comment 2", "bob", "2025-01-02T00:00:00Z"),
        ];
        discussions[0].comments.total_count = 2;

        // Second discussion: no comments (default)

        // Third discussion: 1 comment
        discussions[2].comments.nodes = vec![create_comment(
            "Comment 3",
            "charlie",
            "2025-01-03T00:00:00Z",
        )];
        discussions[2].comments.total_count = 1;

        schema
            .db
            .insert_discussions(discussions, "aicers", "github-dashboard-server")
            .unwrap();

        // Test total comment count and discussion count
        let query = r"
        {
            discussionStat(filter: {}) {
                totalCount
                commentCount
            }
        }";
        let data = schema.execute(query).await.data.into_json().unwrap();
        assert_eq!(data["discussionStat"]["totalCount"], 3);
        assert_eq!(data["discussionStat"]["commentCount"], 3);
    }
}

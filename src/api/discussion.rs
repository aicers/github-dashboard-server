use std::fmt;

use anyhow::Context as AnyhowContext;
use async_graphql::{
    connection::{query, Connection, EmptyFields},
    scalar, Context, Object, Result, SimpleObject,
};

use crate::{
    api::{self, DateTimeUtc},
    database::{self, Database, DiscussionDbSchema, TryFromKeyValue},
    outbound::discussions::ReactionContent,
};

scalar!(ReactionContent);

#[derive(SimpleObject)]
pub(crate) struct Discussion {
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) created_at: DateTimeUtc,
    pub(crate) comments: Vec<DiscussionComment>,
}

impl Discussion {
    pub fn new(owner: String, repo: String, number: i32, schema: DiscussionDbSchema) -> Self {
        Self {
            owner,
            repo,
            number,
            title: schema.title,
            author: schema.author,
            created_at: DateTimeUtc(schema.created_at),
            comments: schema
                .comments
                .nodes
                .into_iter()
                .map(|c| DiscussionComment {
                    author: c.author,
                    created_at: DateTimeUtc(c.created_at),
                })
                .collect(),
        }
    }
}

#[derive(SimpleObject)]
pub(crate) struct DiscussionComment {
    pub(crate) author: String,
    pub(crate) created_at: DateTimeUtc,
}

#[derive(Default)]
pub(super) struct DiscussionQuery;

#[Object]
impl DiscussionQuery {
    async fn discussions(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
    ) -> Result<Connection<String, Discussion, EmptyFields, EmptyFields>> {
        query(
            after,
            before,
            first,
            last,
            |after, before, first, last| async move {
                api::load_connection(ctx, Database::discussions, after, before, first, last)
            },
        )
        .await
    }
}

impl TryFromKeyValue for Discussion {
    fn try_from_key_value(key: &[u8], value: &[u8]) -> anyhow::Result<Self> {
        let (owner, repo, number) = database::parse_key(key)
            .with_context(|| format!("invalid key in database: {key:02x?}"))?;
        let discussion_schema = bincode::deserialize::<DiscussionDbSchema>(value)?;
        let discussion = Discussion::new(owner, repo, number, discussion_schema);
        Ok(discussion)
    }
}

impl fmt::Display for Discussion {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}/{}#{}", self.owner, self.repo, self.number)
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use crate::{
        api::TestSchema,
        database::{
            discussion::{
                Answer, Category, Comment, Comments, Labels, Reaction, Reactions, Replies, Reply,
            },
            DiscussionDbSchema,
        },
        outbound::discussions::ReactionContent,
    };

    #[tokio::test]
    async fn discussion_empty() {
        let schema = TestSchema::new();
        let query = r"
        {
            discussions {
                edges {
                    node {
                        number
                    }
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(res.data.to_string(), "{discussions: {edges: []}}");
    }
    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn discussions_first() {
        let schema = TestSchema::new();
        let date = "2025-06-05T00:00:00Z".parse::<Timestamp>().unwrap();
        let discussions = vec![DiscussionDbSchema {
            number: 123,
            title: "How to use this with API?".to_string(),
            author: "alice".to_string(),
            body: "I'm trying to test this API in my project.".to_string(),
            url: "https://github.com/sample/sample/discussions/123".to_string(),
            created_at: date,
            updated_at: date,
            is_answered: true,
            answer_chosen_at: Some(date),
            answer: Some(Answer {
                body: "You can use the OpenAI API by creating an API key and using the endpoint."
                    .to_string(),
                created_at: date,
                updated_at: date,
                url: "https://github.com/sample/sample/discussions/123#answer".to_string(),
                author: "bob".to_string(),
                replies: Replies {
                    total_count: 1,
                    nodes: vec![Reply {
                        body: "Thanks! That helped.".to_string(),
                        created_at: date,
                        updated_at: date,
                        is_answer: false,
                        author: "alice".to_string(),
                    }],
                },
            }),
            category: Category {
                name: "Q&A".to_string(),
            },
            labels: Some(Labels {
                nodes: vec!["api".to_string(), "help".to_string()],
            }),
            comments: Comments {
                total_count: 2,
                nodes: vec![Comment {
                    body: "Did you check the API docs?".to_string(),
                    author: "charlie".to_string(),
                    created_at: date,
                    updated_at: date,
                    deleted_at: None,
                    is_answer: false,
                    is_minimized: false,
                    last_edited_at: None,
                    published_at: Some(date),
                    reactions: Reactions {
                        total_count: 2,
                        nodes: vec![
                            Reaction {
                                content: ReactionContent::Other("+1".to_string()),
                                created_at: date,
                            },
                            Reaction {
                                content: ReactionContent::Other("heart".to_string()),
                                created_at: date,
                            },
                        ],
                    },
                    replies: Replies {
                        total_count: 1,
                        nodes: vec![Reply {
                            body: "Yes, but I still had some confusion.".to_string(),
                            created_at: date,
                            updated_at: date,
                            is_answer: false,
                            author: "alice".to_string(),
                        }],
                    },
                    upvote_count: 3,
                    url: "https://github.com/sample/sample/discussions/123#comment-1".to_string(),
                }],
            },
            reactions: Reactions {
                total_count: 4,
                nodes: vec![Reaction {
                    content: ReactionContent::Other("thumbs_up".to_string()),
                    created_at: date,
                }],
            },
        }];
        schema
            .db
            .insert_discussions(discussions, "owner", "name")
            .unwrap();

        let query = r"
        {
            discussions(first: 2) {
                edges {
                    node {
                        number
                    }
                }
                pageInfo {
                    hasNextPage
                }
            }
        }";
        let res = schema.execute(query).await;
        assert_eq!(
            res.data.to_string(),
            "{discussions: {edges: [{node: {number: 123}}], pageInfo: {hasNextPage: false}}}"
        );
    }
}

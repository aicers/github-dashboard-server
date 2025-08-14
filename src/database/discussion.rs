use std::num::TryFromIntError;

use anyhow::Result;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{Database, Iter};
use crate::api::Discussion;
use crate::outbound::discussions::{
    DiscussionsRepositoryDiscussionsNodes, DiscussionsRepositoryDiscussionsNodesAnswer,
    DiscussionsRepositoryDiscussionsNodesAnswerAuthor,
    DiscussionsRepositoryDiscussionsNodesAnswerReplies,
    DiscussionsRepositoryDiscussionsNodesAnswerRepliesNodesAuthor,
    DiscussionsRepositoryDiscussionsNodesAuthor, DiscussionsRepositoryDiscussionsNodesComments,
    DiscussionsRepositoryDiscussionsNodesCommentsNodes,
    DiscussionsRepositoryDiscussionsNodesCommentsNodesAuthor,
    DiscussionsRepositoryDiscussionsNodesCommentsNodesReactions,
    DiscussionsRepositoryDiscussionsNodesCommentsNodesReplies,
    DiscussionsRepositoryDiscussionsNodesCommentsNodesRepliesNodesAuthor,
    DiscussionsRepositoryDiscussionsNodesLabels, DiscussionsRepositoryDiscussionsNodesReactions,
    ReactionContent,
};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DiscussionDbSchema {
    pub(crate) number: i32,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) url: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) is_answered: bool,
    pub(crate) answer_chosen_at: Option<Timestamp>,
    pub(crate) answer: Option<Answer>,
    pub(crate) category: Category,
    pub(crate) labels: Option<Labels>,
    pub(crate) comments: Comments,
    pub(crate) reactions: Reactions,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Answer {
    pub(crate) body: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) url: String,
    pub(crate) author: String,
    pub(crate) replies: Replies,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Category {
    pub(crate) name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Labels {
    pub(crate) nodes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Comments {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<Comment>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Comment {
    pub(crate) body: String,
    pub(crate) author: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) deleted_at: Option<Timestamp>,
    pub(crate) is_answer: bool,
    pub(crate) is_minimized: bool,
    pub(crate) last_edited_at: Option<Timestamp>,
    pub(crate) published_at: Option<Timestamp>,
    pub(crate) reactions: Reactions,
    pub(crate) replies: Replies,
    pub(crate) upvote_count: i32,
    pub(crate) url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Reactions {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<Reaction>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reaction {
    pub(crate) content: ReactionContent,
    pub(crate) created_at: Timestamp,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Replies {
    pub(crate) total_count: i32,
    pub(crate) nodes: Vec<Reply>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub(crate) body: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
    pub(crate) is_answer: bool,
    pub(crate) author: String,
}

impl Database {
    pub(crate) fn insert_discussions(
        &self,
        resp: Vec<DiscussionDbSchema>,
        owner: &str,
        repo: &str,
    ) -> Result<()> {
        for item in resp {
            let keystr: String = format!("{owner}/{repo}#{}", item.number);
            Database::insert(&keystr, item, &self.discussion_partition)?;
        }
        Ok(())
    }

    pub(crate) fn discussions(&self, start: Option<&[u8]>, end: Option<&[u8]>) -> Iter<Discussion> {
        let start = start.unwrap_or(b"\x00");
        if let Some(end) = end {
            Iter::new(self.discussion_partition.range(start..end))
        } else {
            Iter::new(self.discussion_partition.range(start..))
        }
    }
}

impl TryFrom<DiscussionsRepositoryDiscussionsNodes> for DiscussionDbSchema {
    type Error = TryFromIntError;

    fn try_from(discussion: DiscussionsRepositoryDiscussionsNodes) -> Result<Self, Self::Error> {
        let author = match discussion.author {
            Some(DiscussionsRepositoryDiscussionsNodesAuthor::User(author)) => author.login,
            _ => String::default(),
        };

        let answer = discussion.answer.map(Answer::try_from).transpose()?;

        Ok(Self {
            number: i32::try_from(discussion.number)?,
            title: discussion.title,
            author,
            body: discussion.body,
            url: discussion.url,
            created_at: discussion.created_at,
            updated_at: discussion.updated_at,
            is_answered: discussion.is_answered.unwrap_or(false),
            answer_chosen_at: discussion.answer_chosen_at,
            answer,
            category: Category {
                name: discussion.category.name,
            },
            labels: discussion.labels.map(Labels::from),
            comments: Comments::try_from(discussion.comments)?,
            reactions: Reactions::try_from(discussion.reactions)?,
        })
    }
}

impl TryFrom<DiscussionsRepositoryDiscussionsNodesAnswer> for Answer {
    type Error = TryFromIntError;

    fn try_from(answer: DiscussionsRepositoryDiscussionsNodesAnswer) -> Result<Self, Self::Error> {
        let author = match answer.author {
            Some(DiscussionsRepositoryDiscussionsNodesAnswerAuthor::User(author)) => author.login,
            _ => String::default(),
        };
        Ok(Self {
            body: answer.body,
            created_at: answer.created_at,
            updated_at: answer.updated_at,
            url: answer.url,
            author,
            replies: Replies::try_from(answer.replies)?,
        })
    }
}

impl TryFrom<DiscussionsRepositoryDiscussionsNodesComments> for Comments {
    type Error = TryFromIntError;

    fn try_from(
        comments: DiscussionsRepositoryDiscussionsNodesComments,
    ) -> Result<Self, Self::Error> {
        let total_count = i32::try_from(comments.total_count)?;

        let nodes = comments
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(Comment::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { total_count, nodes })
    }
}

impl TryFrom<DiscussionsRepositoryDiscussionsNodesCommentsNodes> for Comment {
    type Error = TryFromIntError;

    fn try_from(
        comment: DiscussionsRepositoryDiscussionsNodesCommentsNodes,
    ) -> Result<Self, Self::Error> {
        let author = match comment.author {
            Some(DiscussionsRepositoryDiscussionsNodesCommentsNodesAuthor::User(author)) => {
                author.login
            }
            _ => String::default(),
        };
        Ok(Self {
            body: comment.body,
            author,
            created_at: comment.created_at,
            updated_at: comment.updated_at,
            deleted_at: comment.deleted_at,
            is_answer: comment.is_answer,
            is_minimized: comment.is_minimized,
            last_edited_at: comment.last_edited_at,
            published_at: comment.published_at,
            reactions: Reactions::try_from(comment.reactions)?,
            replies: Replies::try_from(comment.replies)?,
            upvote_count: i32::try_from(comment.upvote_count)?,
            url: comment.url,
        })
    }
}

impl From<DiscussionsRepositoryDiscussionsNodesLabels> for Labels {
    fn from(labels: DiscussionsRepositoryDiscussionsNodesLabels) -> Self {
        let nodes: Vec<String> = labels
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|label| label.name)
            .collect();
        Self { nodes }
    }
}

impl TryFrom<DiscussionsRepositoryDiscussionsNodesReactions> for Reactions {
    type Error = TryFromIntError;

    fn try_from(
        reactions: DiscussionsRepositoryDiscussionsNodesReactions,
    ) -> Result<Self, Self::Error> {
        let nodes = reactions
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|reaction| Reaction {
                content: reaction.content,
                created_at: reaction.created_at,
            })
            .collect();
        Ok(Self {
            total_count: i32::try_from(reactions.total_count)?,
            nodes,
        })
    }
}

impl TryFrom<DiscussionsRepositoryDiscussionsNodesAnswerReplies> for Replies {
    type Error = TryFromIntError;

    fn try_from(
        replies: DiscussionsRepositoryDiscussionsNodesAnswerReplies,
    ) -> Result<Self, Self::Error> {
        let nodes = replies
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|reply| {
                let author = match reply.author {
                    Some(DiscussionsRepositoryDiscussionsNodesAnswerRepliesNodesAuthor::User(
                        author,
                    )) => author.login,
                    _ => String::default(),
                };

                Reply {
                    body: reply.body,
                    created_at: reply.created_at,
                    updated_at: reply.updated_at,
                    is_answer: reply.is_answer,
                    author,
                }
            })
            .collect();
        Ok(Self {
            total_count: i32::try_from(replies.total_count)?,
            nodes,
        })
    }
}

impl TryFrom<DiscussionsRepositoryDiscussionsNodesCommentsNodesReactions> for Reactions {
    type Error = TryFromIntError;

    fn try_from(
        reactions: DiscussionsRepositoryDiscussionsNodesCommentsNodesReactions,
    ) -> Result<Self, Self::Error> {
        let nodes = reactions
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|reaction| Reaction {
                content: reaction.content,
                created_at: reaction.created_at,
            })
            .collect();
        Ok(Self {
            total_count: i32::try_from(reactions.total_count)?,
            nodes,
        })
    }
}

impl TryFrom<DiscussionsRepositoryDiscussionsNodesCommentsNodesReplies> for Replies {
    type Error = TryFromIntError;

    fn try_from(
        replies: DiscussionsRepositoryDiscussionsNodesCommentsNodesReplies,
    ) -> Result<Self, Self::Error> {
        let nodes = replies
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|reply| {
                let author = match reply.author {
                    Some(
                        DiscussionsRepositoryDiscussionsNodesCommentsNodesRepliesNodesAuthor::User(
                            author,
                        ),
                    ) => author.login,
                    _ => String::default(),
                };

                Reply {
                    body: reply.body,
                    created_at: reply.created_at,
                    updated_at: reply.updated_at,
                    is_answer: reply.is_answer,
                    author,
                }
            })
            .collect();
        Ok(Self {
            total_count: i32::try_from(replies.total_count)?,
            nodes,
        })
    }
}

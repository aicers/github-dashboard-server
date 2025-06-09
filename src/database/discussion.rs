use std::fmt;

use anyhow::{Ok, Result};
use serde::{Deserialize, Serialize};

use super::{Database, Iter};
use crate::github::discussions::{
    DiscussionsRepositoryDiscussionsNodes, DiscussionsRepositoryDiscussionsNodesAnswer,
    DiscussionsRepositoryDiscussionsNodesAnswerAuthor,
    DiscussionsRepositoryDiscussionsNodesAnswerReplies,
    DiscussionsRepositoryDiscussionsNodesAnswerRepliesNodesAuthor,
    DiscussionsRepositoryDiscussionsNodesAuthor, DiscussionsRepositoryDiscussionsNodesCategory,
    DiscussionsRepositoryDiscussionsNodesComments,
    DiscussionsRepositoryDiscussionsNodesCommentsNodes,
    DiscussionsRepositoryDiscussionsNodesCommentsNodesAuthor,
    DiscussionsRepositoryDiscussionsNodesCommentsNodesReactions,
    DiscussionsRepositoryDiscussionsNodesCommentsNodesReplies,
    DiscussionsRepositoryDiscussionsNodesCommentsNodesRepliesNodesAuthor,
    DiscussionsRepositoryDiscussionsNodesLabels, DiscussionsRepositoryDiscussionsNodesReactions,
    ReactionContent,
};
use crate::graphql::Discussion;

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscussionDbSchema {
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) url: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) is_answered: bool,
    pub(crate) answer_chosen_at: Option<String>,
    pub(crate) answer: Option<Answer>,
    pub(crate) category: Category,
    pub(crate) labels: Option<Labels>,
    pub(crate) comments: Comments,
    pub(crate) reactions: Reactions,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Answer {
    pub(crate) body: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) url: String,
    pub(crate) author: String,
    pub(crate) replies: Replies,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Category {
    pub(crate) name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Labels {
    pub(crate) nodes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Comments {
    pub(crate) total_count: i64,
    pub(crate) nodes: Vec<Comment>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Comment {
    pub(crate) body: String,
    pub(crate) author: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) deleted_at: Option<String>,
    pub(crate) is_answer: bool,
    pub(crate) is_minimized: bool,
    pub(crate) last_edited_at: Option<String>,
    pub(crate) published_at: Option<String>,
    pub(crate) reactions: Reactions,
    pub(crate) replies: Replies,
    pub(crate) upvote_count: i64,
    pub(crate) url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reactions {
    pub(crate) total_count: i64,
    pub(crate) nodes: Vec<Reaction>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reaction {
    pub(crate) content: String,
    pub(crate) created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Replies {
    pub(crate) total_count: i64,
    pub(crate) nodes: Vec<Reply>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub(crate) body: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
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
            Database::insert(&keystr, item, &self.discussion_tree)?;
        }
        Ok(())
    }

    pub(crate) fn discussions(&self, start: Option<&[u8]>, end: Option<&[u8]>) -> Iter<Discussion> {
        let start = start.unwrap_or(b"\x00");
        if let Some(end) = end {
            self.discussion_tree.range(start..end).into()
        } else {
            self.discussion_tree.range(start..).into()
        }
    }
}

impl fmt::Display for ReactionContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ReactionContent::CONFUSED => "CONFUSED",
            ReactionContent::EYES => "EYES",
            ReactionContent::HEART => "HEART",
            ReactionContent::HOORAY => "HOORAY",
            ReactionContent::LAUGH => "LAUGH",
            ReactionContent::ROCKET => "ROCKET",
            ReactionContent::THUMBS_DOWN => "THUMBS_DOWN",
            ReactionContent::THUMBS_UP => "THUMBS_UP",
            ReactionContent::Other(s) => s,
        };
        write!(f, "{s}")
    }
}

impl From<DiscussionsRepositoryDiscussionsNodes> for DiscussionDbSchema {
    fn from(discussion: DiscussionsRepositoryDiscussionsNodes) -> Self {
        let author = match &discussion.author {
            Some(DiscussionsRepositoryDiscussionsNodesAuthor::User(author)) => author.login.clone(),
            _ => String::default(),
        };

        Self {
            number: discussion.number,
            title: discussion.title,
            author,
            body: discussion.body,
            url: discussion.url,
            created_at: discussion.created_at,
            updated_at: discussion.updated_at,
            is_answered: discussion.is_answered.unwrap_or(false),
            answer_chosen_at: discussion.answer_chosen_at,
            answer: discussion.answer.map(Answer::from),
            category: Category::from(discussion.category),
            labels: discussion.labels.map(Labels::from),
            comments: Comments::from(discussion.comments),
            reactions: Reactions::from(discussion.reactions),
        }
    }
}

impl From<DiscussionsRepositoryDiscussionsNodesCategory> for Category {
    fn from(category: DiscussionsRepositoryDiscussionsNodesCategory) -> Self {
        Self {
            name: (category.name),
        }
    }
}

impl From<DiscussionsRepositoryDiscussionsNodesAnswer> for Answer {
    fn from(answer: DiscussionsRepositoryDiscussionsNodesAnswer) -> Self {
        let author = match &answer.author {
            Some(DiscussionsRepositoryDiscussionsNodesAnswerAuthor::User(author)) => {
                author.login.clone()
            }
            _ => String::default(),
        };
        Self {
            body: answer.body,
            created_at: answer.created_at,
            updated_at: answer.updated_at,
            url: answer.url,
            author,
            replies: Replies::from(answer.replies),
        }
    }
}

impl From<DiscussionsRepositoryDiscussionsNodesComments> for Comments {
    fn from(comments: DiscussionsRepositoryDiscussionsNodesComments) -> Self {
        let nodes = if let Some(comments) = comments.nodes {
            comments.into_iter().flatten().map(Comment::from).collect()
        } else {
            vec![]
        };

        Self {
            total_count: comments.total_count,
            nodes,
        }
    }
}

impl From<DiscussionsRepositoryDiscussionsNodesCommentsNodes> for Comment {
    fn from(comment: DiscussionsRepositoryDiscussionsNodesCommentsNodes) -> Self {
        let author = match &comment.author {
            Some(DiscussionsRepositoryDiscussionsNodesCommentsNodesAuthor::User(author)) => {
                author.login.clone()
            }
            _ => String::default(),
        };
        Self {
            body: comment.body,
            author,
            created_at: comment.created_at,
            updated_at: comment.updated_at,
            deleted_at: comment.deleted_at,
            is_answer: comment.is_answer,
            is_minimized: comment.is_minimized,
            last_edited_at: comment.last_edited_at,
            published_at: comment.published_at,
            reactions: Reactions::from(comment.reactions),
            replies: Replies::from(comment.replies),
            upvote_count: comment.upvote_count,
            url: comment.url,
        }
    }
}

impl From<DiscussionsRepositoryDiscussionsNodesLabels> for Labels {
    fn from(labels: DiscussionsRepositoryDiscussionsNodesLabels) -> Self {
        let nodes: Vec<String> = if let Some(labels) = labels.nodes {
            labels
                .into_iter()
                .flatten()
                .map(|label| label.name)
                .collect()
        } else {
            vec![]
        };
        Self { nodes }
    }
}

impl From<DiscussionsRepositoryDiscussionsNodesReactions> for Reactions {
    fn from(reactions: DiscussionsRepositoryDiscussionsNodesReactions) -> Self {
        let nodes = if let Some(reactions) = reactions.nodes {
            reactions
                .into_iter()
                .flatten()
                .map(|reaction| Reaction {
                    content: reaction.content.to_string(),
                    created_at: reaction.created_at,
                })
                .collect()
        } else {
            vec![]
        };
        Self {
            total_count: reactions.total_count,
            nodes,
        }
    }
}

impl From<DiscussionsRepositoryDiscussionsNodesAnswerReplies> for Replies {
    fn from(replies: DiscussionsRepositoryDiscussionsNodesAnswerReplies) -> Self {
        let nodes = if let Some(replies) = replies.nodes {
            replies
                .into_iter()
                .flatten()
                .map(|reply| {
                    let author = match &reply.author {
                        Some(
                            DiscussionsRepositoryDiscussionsNodesAnswerRepliesNodesAuthor::User(
                                author,
                            ),
                        ) => author.login.clone(),
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
                .collect()
        } else {
            vec![]
        };
        Self {
            total_count: replies.total_count,
            nodes,
        }
    }
}

impl From<DiscussionsRepositoryDiscussionsNodesCommentsNodesReactions> for Reactions {
    fn from(reactions: DiscussionsRepositoryDiscussionsNodesCommentsNodesReactions) -> Self {
        let nodes = if let Some(reactions) = reactions.nodes {
            reactions
                .into_iter()
                .flatten()
                .map(|reaction| Reaction {
                    content: reaction.content.to_string(),
                    created_at: reaction.created_at,
                })
                .collect()
        } else {
            vec![]
        };
        Self {
            total_count: reactions.total_count,
            nodes,
        }
    }
}

impl From<DiscussionsRepositoryDiscussionsNodesCommentsNodesReplies> for Replies {
    fn from(replies: DiscussionsRepositoryDiscussionsNodesCommentsNodesReplies) -> Self {
        let nodes = if let Some(replies) = replies.nodes {
            replies
                .into_iter()
                .flatten()
                .map(|reply| {
                    let author = match &reply.author {
                        Some(
                            DiscussionsRepositoryDiscussionsNodesCommentsNodesRepliesNodesAuthor::User(
                                author,
                            ),
                        ) => author.login.clone(),
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
                .collect()
        } else {
            vec![]
        };
        Self {
            total_count: replies.total_count,
            nodes,
        }
    }
}

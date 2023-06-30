use bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use crate::relative_time;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Comment {
    pub _id: ObjectId,
    pub index: i64,
    pub user_id: ObjectId,
    pub body: String,
    pub created_at: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Issue {
    pub user_id: ObjectId,
    pub index: i64,
    pub title: String,
    pub body: String,
    pub comments: Vec<Comment>,
    pub visibility: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: u8,
}

impl Issue {
    pub fn created_at(&self) -> String {
        relative_time::to_string(self.created_at)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub _id: ObjectId,
    pub user_id: ObjectId,
    pub name: String,
    pub description: String,
    pub visibility: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub issues: Vec<Issue>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub _id: ObjectId,
    pub email: String,
    pub username: String,
    pub password: String,
    pub salt: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub log: Vec<Log>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    RepositoryCreate,
}

impl Event {
    pub fn to_string(&self) -> String {
        match self {
            Event::RepositoryCreate => String::from("repository.create"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    pub event: String,
    pub description: String,
    pub created_at: i64,
}

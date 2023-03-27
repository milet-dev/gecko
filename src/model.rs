use bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub user_id: ObjectId,
    pub name: String,
    pub description: String,
    pub visibility: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub _id: ObjectId,
    pub email: String,
    pub username: String,
    pub password: String,
    pub created_at: i64,
    pub updated_at: i64,
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct User {
    pub email: String,
    pub username: String,
    pub password: String,
    pub repositories: Vec<Repository>,
}

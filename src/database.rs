use crate::model::{Repository, User};
use bson::oid::ObjectId;
use futures::TryStreamExt;
use mongodb::options::{FindOneOptions, FindOptions};

#[derive(Clone)]
pub struct Database {
    inner: mongodb::Database,
}

impl Database {
    pub fn new(client: &mongodb::Client) -> Self {
        Self {
            inner: client.database("gecko"),
        }
    }

    pub async fn login(&self, username: &str, password: &str) -> Option<User> {
        let collection = self.inner.collection::<User>("users");
        let result = collection
            .find_one(
                bson::doc! { "username": username, "password": password },
                None,
            )
            .await;
        result.unwrap_or(None)
    }

    pub async fn find_user(&self, username: &str) -> Option<User> {
        let collection = self.inner.collection::<User>("users");
        let result = collection
            .find_one(bson::doc! { "username": username }, None)
            .await;
        result.unwrap_or(None)
    }

    pub async fn find_user_repositories(&self, username: &str) -> Option<Vec<Repository>> {
        let Some(user) = self.find_user(username).await else {
            return None;
        };

        let collection = self.inner.collection::<Repository>("repositories");
        let find_options = FindOptions::builder()
            .projection(bson::doc! { "user_id": ObjectId::default(), "name": 1, "description": 1, "visibility": 1 })
            .build();
        let result = collection
            .find(bson::doc! { "user_id": user._id }, find_options)
            .await;
        let Ok(cursor) = result else {
            return None;
        };
        cursor.try_collect::<Vec<_>>().await.ok()
    }

    pub async fn find_repository(&self, user: &Option<User>, name: &str) -> Option<Repository> {
        let filter = match user {
            Some(user) => bson::doc! { "user_id": user._id, "name": name },
            None => bson::doc! { "name": name },
        };
        let collection = self.inner.collection::<Repository>("repositories");
        let find_options = FindOneOptions::builder()
            .projection(bson::doc! { "user_id": ObjectId::default(), "name": 1, "description": 1, "visibility": 1 })
            .build();
        let result = collection.find_one(filter, find_options).await;
        result.unwrap_or(None)
    }

    pub async fn new_repository(
        &self,
        user: &Option<User>,
        name: &str,
        description: Option<&str>,
        visibility: &str,
    ) {
        let Some(user) = user else {
            panic!();
        };
        let collection = self.inner.collection::<Repository>("repositories");
        let repository = Repository {
            user_id: user._id,
            name: name.to_owned(),
            description: description.unwrap_or("default").to_owned(),
            visibility: visibility.to_owned(),
        };
        if collection.insert_one(&repository, None).await.is_err() {
            todo!();
        }
    }
}

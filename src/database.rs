use std::str::FromStr;

use crate::model::{Event, Log, Repository, User};
use bson::oid::ObjectId;
use futures::TryStreamExt;
use mongodb::options::{FindOneOptions, FindOptions};

#[derive(Clone)]
pub struct Database {
    inner: mongodb::Database,
}

impl Database {
    pub fn new(client: &mongodb::Client, name: &str) -> Self {
        Self {
            inner: client.database(name),
        }
    }

    pub async fn login(&self, username: &str, password: &str) -> Option<User> {
        let collection = self.inner.collection::<User>("users");
        let result = collection
            .find_one(bson::doc! { "username": username }, None)
            .await
            .unwrap();
        result.filter(|user| {
            let hash = blake3::hash(format!("{password}{}", &user.salt).as_bytes()).to_string();
            hash == user.password
        })
    }

    pub async fn find_user(&self, username: &str) -> Option<User> {
        let collection = self.inner.collection::<User>("users");
        let result = collection
            .find_one(bson::doc! { "username": username }, None)
            .await;
        result.unwrap_or(None)
    }

    pub async fn find_user_from_id(&self, id: &str) -> Option<User> {
        let collection = self.inner.collection::<User>("users");
        let result = collection
            .find_one(bson::doc! { "_id": ObjectId::from_str(id).unwrap() }, None)
            .await;
        result.unwrap_or(None)
    }

    pub async fn find_user_repositories(&self, id: ObjectId) -> Option<Vec<Repository>> {
        let Some(user) = self.find_user_from_id(&id.to_string()).await else {
            return None;
        };

        let collection = self.inner.collection::<Repository>("repositories");
        let find_options = FindOptions::builder()
            .projection(bson::doc! { "user_id": ObjectId::default(), "name": 1, "description": 1, "visibility": 1, "created_at": 1, "updated_at": 1, "issues": 1 })
            .build();
        let result = collection
            .find(bson::doc! { "user_id": user._id }, find_options)
            .await;
        let Ok(cursor) = result else {
            return None;
        };
        cursor.try_collect::<Vec<_>>().await.ok()
    }

    pub async fn find_repository(&self, user: Option<&User>, name: &str) -> Option<Repository> {
        let filter = match user {
            Some(user) => bson::doc! { "user_id": user._id, "name": name },
            None => bson::doc! { "name": name },
        };
        let collection = self.inner.collection::<Repository>("repositories");
        let find_options = FindOneOptions::builder()
            .projection(bson::doc! { "_id": 1, "user_id": 1, "name": 1, "description": 1, "visibility": 1, "created_at": 1, "updated_at": 1, "issues": 1 })
            .build();
        let result = collection.find_one(filter, find_options).await;
        result.unwrap_or(None)
    }

    pub async fn new_repository(
        &self,
        user: Option<&User>,
        name: &str,
        description: Option<String>,
        visibility: &str,
    ) -> anyhow::Result<(), Error> {
        let Some(user) = user else {
            panic!();
        };

        let collection = self.inner.collection::<Repository>("repositories");

        if let Ok(document) = collection
            .find_one(bson::doc! { "user_id": user._id, "name": name }, None)
            .await
        {
            if document.is_some() {
                return Err(Error::Found);
            }
        }

        let now = time::OffsetDateTime::now_utc();
        let unix_timestamp = now.unix_timestamp();
        let repository = Repository {
            _id: ObjectId::new(),
            user_id: user._id,
            name: name.to_owned(),
            description: description.unwrap_or_default(),
            visibility: visibility.to_owned(),
            created_at: unix_timestamp,
            updated_at: unix_timestamp,
            issues: vec![],
        };
        if collection.insert_one(&repository, None).await.is_err() {
            todo!();
        }

        Ok(())
    }

    pub async fn delete_repository(
        &self,
        user: &Option<User>,
        name: &str,
    ) -> anyhow::Result<(), Error> {
        let Some(user) = user else {
            return Err(Error::Unauthorized);
        };
        let collection = self.inner.collection::<Repository>("repositories");
        let result = collection
            .find_one_and_delete(bson::doc! { "user_id": user._id, "name": name}, None)
            .await
            .unwrap();
        if result.is_none() {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub async fn add_user_log(&self, user: &User, event: Event, description: Option<String>) {
        let now = time::OffsetDateTime::now_utc();
        let unix_timestamp = now.unix_timestamp();
        let log = Log {
            event: event.to_str().to_owned(),
            description: description.unwrap_or("undefined".to_owned()),
            created_at: unix_timestamp,
        };
        let users = self.inner.collection::<User>("users");
        let result = users
            .update_one(
                bson::doc! {"_id": user._id},
                bson::doc! {
                    "$push": {
                        "log": {
                            "event": log.event,
                            "description": log.description,
                            "created_at": log.created_at,
                        }
                    }
                },
                None,
            )
            .await;
        debug_assert!(result.is_ok());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Unauthorized,
    NotFound,
    Found,
}

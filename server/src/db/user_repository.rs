use crate::error::Result;
use domain::user::{Role, User, UserSummary};
use sqlx::Pool;

pub struct UserRepository {
    pool: Pool<sqlx::Postgres>,
}

pub struct UserRow {
    pub id: uuid::Uuid,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl UserRow {
    pub fn into_user(self, roles: Vec<Role>) -> User {
        User {
            id: self.id,
            username: self.username,
            display_name: self.display_name,
            password_hash: self.password_hash,
            is_active: self.is_active,
            roles,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl UserRepository {
    pub fn new(pool: Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }

    pub async fn list_active(&self) -> Result<Vec<UserSummary>> {
        sqlx::query_as!(
            UserSummary,
            r#"
            SELECT
                id,
                username,
                display_name,
                is_active
            FROM users
            WHERE is_active = TRUE
            ORDER BY display_name
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.into())
    }

    pub async fn get_by_username(&self, username: &str) -> Result<Option<User>> {
        let Some(user) = sqlx::query_as!(
            UserRow,
            r#"
        SELECT
            id,
            username,
            display_name,
            password_hash,
            is_active,
            created_at,
            updated_at
        FROM users
        WHERE username = $1
        "#,
            username
        )
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let roles = sqlx::query_as!(
            Role,
            r#"
        SELECT
            r.id,
            r.name,
            r.description,
            r.created_at
        FROM roles r
        INNER JOIN user_roles ur
            ON ur.role_id = r.id
        WHERE ur.user_id = $1
        ORDER BY r.name
        "#,
            user.id
        )
        .fetch_all(&self.pool)
        .await?;


        Ok(Some(user.into_user(roles)))
    }
}

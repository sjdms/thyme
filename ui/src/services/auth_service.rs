//! Login business logic on top of `services::connection`.
//!
//! This file never constructs a `FramedStream`, holds a raw socket, or
//! touches a `packets::*` type directly — it only sees the higher-level
//! types `connection` exposes (`PendingConnection`, `LoginOutcome`,
//! `Connection`, `AvailableUser`).

use packets::hello::HelloOperationMode;

use crate::error::Result;
use crate::services::connection::{AvailableUser, Connection, LoginOutcome, PendingConnection};

/// What the login screen should show after a connect/login attempt.
pub enum AuthAttempt {
    /// Connected to the server and awaiting credentials.
    AwaitingCredentials { session: PendingConnection, available_users: Vec<AvailableUser> },
    /// Credentials were rejected but the server is still accepting more
    /// attempts.
    Rejected { session: PendingConnection, available_users: Vec<AvailableUser>, message: String },
    /// Authenticated; `Connection` is ready to hand off to the rest of
    /// the app. `roles` are the authenticated user's role names (e.g.
    /// `"doctor"`) — `controllers::login` uses these, not a
    /// client-declared terminal mode, to decide which dashboard window
    /// to open.
    Authenticated { connection: Connection, roles: Vec<String> },
}

pub struct AuthService {
    server_addr: String,
    computer_name: String,
}

impl AuthService {
    pub fn new(server_addr: impl Into<String>, computer_name: impl Into<String>) -> Self {
        Self { server_addr: server_addr.into(), computer_name: computer_name.into() }
    }

    /// Opens a TCP connection and performs the `Hello` handshake. The
    /// `HelloOperationMode` here just identifies this terminal for the
    /// server's own bookkeeping (`ThymeServer::active_clients`) — it is
    /// sent before any user has authenticated, so it can't reflect the
    /// eventual user's role. Which dashboard window actually opens is
    /// decided afterward, from the authenticated user's roles (see
    /// `AuthAttempt::Authenticated`), per THYME_UI_SPEC.md §62
    /// Role-specific UI.
    pub async fn connect(&self) -> Result<AuthAttempt> {
        let session = PendingConnection::connect(
            &self.server_addr,
            self.computer_name.clone(),
            HelloOperationMode::Receptionist,
        )
        .await?;

        let available_users = session.available_users().to_vec();
        Ok(AuthAttempt::AwaitingCredentials { session, available_users })
    }

    /// Submits credentials against an existing session.
    pub async fn login(&self, session: PendingConnection, username: String, password: String) -> Result<AuthAttempt> {
        match session.try_login(username, &password).await? {
            LoginOutcome::Authenticated { connection, roles } => {
                Ok(AuthAttempt::Authenticated { connection, roles })
            }
            LoginOutcome::Rejected { retry, message } => {
                let available_users = retry.available_users().to_vec();
                Ok(AuthAttempt::Rejected { session: retry, available_users, message })
            }
        }
    }
}
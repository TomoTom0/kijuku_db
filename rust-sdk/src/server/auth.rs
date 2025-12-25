use rand::Rng;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub created_at: u64,
}

pub struct SessionStore {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create(&self) -> String {
        let session_id = Uuid::new_v4().to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch, this should not happen.")
            .as_secs();

        let session = Session {
            id: session_id.clone(),
            created_at: now,
        };

        self.sessions
            .lock()
            .unwrap()
            .insert(session_id.clone(), session);

        session_id
    }

    pub fn exists(&self, session_id: &str) -> bool {
        self.sessions.lock().unwrap().contains_key(session_id)
    }

    pub fn delete(&self, session_id: &str) {
        self.sessions.lock().unwrap().remove(session_id);
    }

    pub fn cleanup(&self, max_age_secs: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch, this should not happen.")
            .as_secs();

        self.sessions
            .lock()
            .unwrap()
            .retain(|_, session| now - session.created_at <= max_age_secs);
    }
}

pub fn generate_password() -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const LENGTH: usize = 12;

    let mut rng = rand::thread_rng();
    (0..LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARS.len());
            CHARS[idx] as char
        })
        .collect()
}

pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub struct AuthManager {
    password: String,
    password_hash: String,
    session_store: SessionStore,
}

impl AuthManager {
    pub fn new(password: String) -> Self {
        let password_hash = hash_password(&password);

        Self {
            password,
            password_hash,
            session_store: SessionStore::new(),
        }
    }

    pub fn get_password(&self) -> &str {
        &self.password
    }

    pub fn authenticate(&self, password: &str) -> Option<String> {
        let hash = hash_password(password);
        if hash == self.password_hash {
            Some(self.session_store.create())
        } else {
            None
        }
    }

    pub fn validate_session(&self, session_id: &str) -> bool {
        self.session_store.exists(session_id)
    }

    pub fn logout(&self, session_id: &str) {
        self.session_store.delete(session_id);
    }

    pub fn cleanup_sessions(&self, max_age_secs: u64) {
        self.session_store.cleanup(max_age_secs);
    }
}

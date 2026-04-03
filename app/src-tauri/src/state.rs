use rustygene_api::ServerHandle;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[derive(Clone)]
pub struct RuntimeState {
    pub api_port: Arc<RwLock<Option<u16>>>,
    pub server_handle: Arc<Mutex<Option<ServerHandle>>>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            api_port: Arc::new(RwLock::new(None)),
            server_handle: Arc::new(Mutex::new(None)),
        }
    }
}

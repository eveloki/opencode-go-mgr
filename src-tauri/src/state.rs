use ocg_core::state::CoreState;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::process::Child;
use std::sync::Arc;

#[derive(Default)]
pub struct BrowserProcessState {
    pub children: HashMap<String, Vec<Child>>,
}

pub struct GuiState {
    pub core: CoreState,
    pub browser_processes: Arc<Mutex<BrowserProcessState>>,
}

pub type AppState = Arc<GuiState>;

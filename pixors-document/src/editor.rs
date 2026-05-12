use super::session::Session;
use super::tab::SessionId;

pub struct EditorState {
    pub sessions: Vec<Session>,
    pub active: Option<SessionId>,
    pub next_session_id: u64,
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            active: None,
            next_session_id: 0,
        }
    }

    pub fn alloc_session_id(&mut self) -> SessionId {
        let id = SessionId(self.next_session_id);
        self.next_session_id += 1;
        id
    }

    pub fn push(&mut self, session: Session) {
        let id = session.id;
        let title = session
            .document
            .assets
            .primary_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string();
        self.sessions.push(session);
        self.active = Some(id);
        tracing::info!(
            "[state] push id={id:?} title=\"{title}\" count={} active={id:?}",
            self.sessions.len(),
        );
    }

    pub fn close(&mut self, id: SessionId) {
        if let Some(pos) = self.sessions.iter().position(|s| s.id == id) {
            self.sessions[pos].transient.cleanup_disk_caches();
            let title = self.sessions[pos]
                .document
                .assets
                .primary_path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("untitled")
                .to_string();
            self.sessions.remove(pos);
            let old_active = self.active;
            if self.active == Some(id) {
                self.active = self
                    .sessions
                    .get(pos)
                    .or_else(|| self.sessions.get(pos.saturating_sub(1)))
                    .map(|s| s.id);
            }
            tracing::info!(
                "[state] close id={id:?} title=\"{title}\" active {:?}→{:?} count={}",
                old_active,
                self.active,
                self.sessions.len(),
            );
        } else {
            tracing::warn!("[state] close id={id:?} not found");
        }
    }

    pub fn switch(&mut self, id: SessionId) {
        let old = self.active;
        self.active = Some(id);
        if old != self.active {
            let title = self
                .session(id)
                .and_then(|s| s.document.assets.primary_path.as_ref())
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("untitled");
            tracing::info!("[state] switch {:?}→{id:?} title=\"{title}\"", old);
        }
    }

    pub fn swap(&mut self, a: usize, b: usize) {
        if a < self.sessions.len() && b < self.sessions.len() {
            self.sessions.swap(a, b);
        }
    }

    pub fn active_session(&self) -> Option<&Session> {
        self.active.and_then(|id| self.session(id))
    }

    pub fn active_session_mut(&mut self) -> Option<&mut Session> {
        self.active.and_then(|id| self.session_mut(id))
    }

    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn session_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn all_sessions(&self) -> &[Session] {
        &self.sessions
    }

    pub fn active_id(&self) -> Option<SessionId> {
        self.active
    }
}

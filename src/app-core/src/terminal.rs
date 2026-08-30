use nodeinnet_p2p::P2pMessage;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TerminalState {
    pub alive: bool,
    pub resource_id: Option<String>,
    pub rows: u16,
    pub cols: u16,
}

impl Default for TerminalState {
    fn default() -> Self {
        Self {
            alive: false,
            resource_id: None,
            rows: 24,
            cols: 80,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    TerminalChanged { terminal: TerminalState },
}

pub struct TerminalRouter {
    active: Mutex<Option<String>>,
    tx: broadcast::Sender<Vec<u8>>,
}

impl std::fmt::Debug for TerminalRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalRouter")
            .field("active", &*self.active.lock().unwrap())
            .finish_non_exhaustive()
    }
}

impl TerminalRouter {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            active: Mutex::new(None),
            tx: broadcast::channel(256).0,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.tx.subscribe()
    }

    pub fn route(&self, msg: P2pMessage) -> Option<P2pMessage> {
        match msg {
            P2pMessage::TerminalOutput { resource_id, data }
                if self.active.lock().unwrap().as_deref() == Some(resource_id.as_str()) =>
            {
                let _ = self.tx.send(data);
                None
            }
            other => Some(other),
        }
    }

    fn set_active(&self, id: Option<String>) {
        *self.active.lock().unwrap() = id;
    }
}

type Transport = (String, Box<dyn Fn(P2pMessage)>);

pub struct Terminal {
    router: Arc<TerminalRouter>,
    transport: Option<Transport>,
    state: TerminalState,
    events: Vec<Event>,
}

impl Default for Terminal {
    fn default() -> Self {
        Self::with_router(TerminalRouter::new())
    }
}

impl Terminal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_router(router: Arc<TerminalRouter>) -> Self {
        Self {
            router,
            transport: None,
            state: TerminalState::default(),
            events: Vec::new(),
        }
    }

    pub fn router(&self) -> Arc<TerminalRouter> {
        self.router.clone()
    }

    pub fn set_resource(
        &mut self,
        resource_id: impl Into<String>,
        send: impl Fn(P2pMessage) + 'static,
    ) {
        if self.state.alive {
            self.stop();
        }
        let resource_id = resource_id.into();
        self.state.resource_id = Some(resource_id.clone());
        self.transport = Some((resource_id, Box::new(send)));
        self.emit();
    }

    pub fn start(&mut self) -> bool {
        let Some((id, send)) = &self.transport else {
            return false;
        };
        self.router.set_active(Some(id.clone()));
        send(P2pMessage::StartTerminal {
            resource_id: id.clone(),
        });
        send(P2pMessage::TerminalResize {
            resource_id: id.clone(),
            rows: self.state.rows,
            cols: self.state.cols,
        });
        self.state.alive = true;
        self.emit();
        true
    }

    pub fn input(&mut self, data: Vec<u8>) -> bool {
        if !self.state.alive {
            return false;
        }
        let Some((id, send)) = &self.transport else {
            return false;
        };
        send(P2pMessage::TerminalInput {
            resource_id: id.clone(),
            data,
        });
        true
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> bool {
        if !self.state.alive {
            return false;
        }
        let Some((id, send)) = &self.transport else {
            return false;
        };
        send(P2pMessage::TerminalResize {
            resource_id: id.clone(),
            rows,
            cols,
        });
        self.state.rows = rows;
        self.state.cols = cols;
        self.emit();
        true
    }

    pub fn stop(&mut self) -> bool {
        if !self.state.alive {
            return false;
        }
        if let Some((id, send)) = &self.transport {
            send(P2pMessage::StopTerminal {
                resource_id: id.clone(),
            });
        }
        self.router.set_active(None);
        self.state.alive = false;
        self.emit();
        true
    }

    pub fn unwire(&mut self) {
        self.stop();
        if self.transport.is_none() && self.state.resource_id.is_none() {
            return;
        }
        self.transport = None;
        self.state.resource_id = None;
        self.emit();
    }

    pub fn state(&self) -> &TerminalState {
        &self.state
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn emit(&mut self) {
        self.events.push(Event::TerminalChanged {
            terminal: self.state.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn rig() -> (Terminal, Rc<RefCell<Vec<P2pMessage>>>) {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let mut term = Terminal::new();
        let s = sent.clone();
        term.set_resource("term-1", move |msg| s.borrow_mut().push(msg));
        term.take_events();
        (term, sent)
    }

    fn output(id: &str, data: &[u8]) -> P2pMessage {
        P2pMessage::TerminalOutput {
            resource_id: id.into(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn start_sends_start_then_resize_and_claims_output() {
        let (mut term, sent) = rig();
        let mut rx = term.router().subscribe();

        assert!(term.start());
        assert!(term.state().alive);
        assert!(matches!(sent.borrow()[0], P2pMessage::StartTerminal { .. }));
        assert!(matches!(
            sent.borrow()[1],
            P2pMessage::TerminalResize {
                rows: 24,
                cols: 80,
                ..
            }
        ));
        assert_eq!(
            term.take_events().last(),
            Some(&Event::TerminalChanged {
                terminal: term.state().clone()
            })
        );

        assert!(term.router().route(output("term-1", b"hi")).is_none());
        assert_eq!(rx.try_recv().unwrap(), b"hi");
    }

    #[test]
    fn input_flows_only_while_alive() {
        let (mut term, sent) = rig();
        assert!(
            !term.input(b"early".to_vec()),
            "input before start is dropped"
        );
        assert!(sent.borrow().is_empty());

        term.start();
        assert!(term.input(b"ls\n".to_vec()));
        assert!(matches!(
            sent.borrow().last().unwrap(),
            P2pMessage::TerminalInput { data, .. } if data == b"ls\n"
        ));

        term.stop();
        assert!(!term.input(b"late".to_vec()), "input after stop is dropped");
    }

    #[test]
    fn resize_updates_state_and_wire() {
        let (mut term, sent) = rig();
        assert!(!term.resize(50, 120), "resize before start is a no-op");
        term.start();
        assert!(term.resize(50, 120));
        assert_eq!((term.state().rows, term.state().cols), (50, 120));
        assert!(matches!(
            sent.borrow().last().unwrap(),
            P2pMessage::TerminalResize {
                rows: 50,
                cols: 120,
                ..
            }
        ));
    }

    #[test]
    fn stop_releases_the_router() {
        let (mut term, sent) = rig();
        term.start();
        assert!(term.stop());
        assert!(!term.state().alive);
        assert!(matches!(
            sent.borrow().last().unwrap(),
            P2pMessage::StopTerminal { .. }
        ));
        assert!(term.router().route(output("term-1", b"zombie")).is_some());
    }

    #[test]
    fn router_never_delivers_to_the_wrong_session() {
        let (mut term, _) = rig();
        term.start();
        assert!(term.router().route(output("other-term", b"x")).is_some());
        assert!(term.router().route(P2pMessage::Goodbye).is_some());
    }

    #[test]
    fn unwire_stops_the_session_and_drops_the_peer() {
        let (mut term, sent) = rig();
        term.start();
        term.unwire();
        assert!(!term.state().alive);
        assert!(term.state().resource_id.is_none());
        assert!(matches!(
            sent.borrow().last().unwrap(),
            P2pMessage::StopTerminal { .. }
        ));
        assert!(!term.start(), "an unwired terminal has nothing to start");
        assert!(term.router().route(output("term-1", b"zombie")).is_some());
    }

    #[test]
    fn switching_resource_stops_the_old_session() {
        let (mut term, sent) = rig();
        term.start();
        term.set_resource("term-2", |_| {});
        assert!(!term.state().alive);
        assert!(
            matches!(sent.borrow().last().unwrap(), P2pMessage::StopTerminal { resource_id } if resource_id == "term-1"),
            "old session stopped through the OLD transport"
        );
        assert_eq!(term.state().resource_id.as_deref(), Some("term-2"));
    }
}

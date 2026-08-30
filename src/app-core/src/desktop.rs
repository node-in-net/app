use nodeinnet_p2p::{DesktopInputEvent, P2pMessage};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct DesktopMetrics {
    pub frames_total: u64,
    pub bytes_total: u64,
    pub first_frame_unix_ms: Option<u64>,
    pub last_frame_unix_ms: Option<u64>,
    pub stream_stops: u32,
    pub last_width: u32,
    pub last_height: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DesktopStats {
    #[serde(flatten)]
    pub metrics: DesktopMetrics,
    pub time_to_first_frame_ms: Option<u64>,
    pub avg_fps: f64,
    pub ms_since_last_frame: Option<u64>,
}

#[derive(Default)]
struct RouterInner {
    started_unix_ms: Option<u64>,
    metrics: DesktopMetrics,
    frame: Option<(u32, u32, Vec<u8>)>,
    screen: Option<(usize, usize)>,
}

pub struct DesktopRouter {
    inner: Mutex<RouterInner>,
}

impl std::fmt::Debug for DesktopRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopRouter").finish_non_exhaustive()
    }
}

impl DesktopRouter {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(RouterInner::default()),
        })
    }

    pub fn set_screen_size(&self, width: usize, height: usize) {
        if width == 0 || height == 0 {
            return;
        }
        self.inner.lock().unwrap().screen = Some((width, height));
    }

    pub fn screen_size(&self) -> Option<(usize, usize)> {
        self.inner.lock().unwrap().screen
    }

    pub fn record_frame(&self, width: u32, height: u32, compressed_len: usize, bgra: &[u8]) {
        let now = now_ms();
        let mut inner = self.inner.lock().unwrap();
        let m = &mut inner.metrics;
        m.frames_total += 1;
        m.bytes_total += compressed_len as u64;
        m.first_frame_unix_ms.get_or_insert(now);
        m.last_frame_unix_ms = Some(now);
        m.last_width = width;
        m.last_height = height;
        inner.frame = Some((width, height, bgra.to_vec()));
    }

    pub fn record_stop(&self) {
        self.inner.lock().unwrap().metrics.stream_stops += 1;
    }

    pub fn mark_started(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.metrics = DesktopMetrics::default();
        inner.frame = None;
        inner.screen = None;
        inner.started_unix_ms = Some(now_ms());
    }

    pub fn stats(&self) -> DesktopStats {
        let inner = self.inner.lock().unwrap();
        let m = inner.metrics.clone();
        let now = now_ms();
        let time_to_first_frame_ms = match (inner.started_unix_ms, m.first_frame_unix_ms) {
            (Some(start), Some(first)) => Some(first.saturating_sub(start)),
            _ => None,
        };
        let avg_fps = match (m.first_frame_unix_ms, m.last_frame_unix_ms) {
            (Some(first), Some(last)) if last > first && m.frames_total > 1 => {
                (m.frames_total - 1) as f64 * 1000.0 / (last - first) as f64
            }
            _ => 0.0,
        };
        let ms_since_last_frame = m.last_frame_unix_ms.map(|t| now.saturating_sub(t));
        DesktopStats {
            metrics: m,
            time_to_first_frame_ms,
            avg_fps,
            ms_since_last_frame,
        }
    }

    pub fn latest_frame(&self) -> Option<(u32, u32, Vec<u8>)> {
        self.inner.lock().unwrap().frame.clone()
    }

    pub fn frame_seq(&self) -> u64 {
        self.inner.lock().unwrap().metrics.frames_total
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopState {
    pub connected: bool,
    pub controlling: bool,
    pub resource_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    DesktopChanged { desktop: DesktopState },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamOptions {
    pub original_size: bool,
    pub bitrate_bps: Option<u32>,
    pub force_select: bool,
}

struct Wiring {
    resource_id: String,
    send: Box<dyn Fn(P2pMessage)>,
    media: Box<dyn Fn(bool, StreamOptions)>,
}

pub struct Desktop {
    router: Arc<DesktopRouter>,
    wiring: Option<Wiring>,
    state: DesktopState,
    opts: StreamOptions,
    events: Vec<Event>,
}

impl Default for Desktop {
    fn default() -> Self {
        Self::with_router(DesktopRouter::new())
    }
}

impl Desktop {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_router(router: Arc<DesktopRouter>) -> Self {
        Self {
            router,
            wiring: None,
            state: DesktopState::default(),
            opts: StreamOptions::default(),
            events: Vec::new(),
        }
    }

    pub fn router(&self) -> Arc<DesktopRouter> {
        self.router.clone()
    }

    pub fn set_resource(
        &mut self,
        resource_id: impl Into<String>,
        send: impl Fn(P2pMessage) + 'static,
        media: impl Fn(bool, StreamOptions) + 'static,
    ) {
        if self.state.connected {
            self.disconnect();
        }
        let resource_id = resource_id.into();
        self.state.resource_id = Some(resource_id.clone());
        self.wiring = Some(Wiring {
            resource_id,
            send: Box::new(send),
            media: Box::new(media),
        });
        self.emit();
    }

    pub fn connect(&mut self, opts: StreamOptions) -> bool {
        let Some(w) = &self.wiring else {
            return false;
        };
        self.opts = opts;
        self.router.mark_started();
        (w.media)(true, opts);
        self.state.connected = true;
        self.emit();
        true
    }

    pub fn set_stream_options(&mut self, opts: StreamOptions) -> bool {
        if self.opts == opts {
            return false;
        }
        self.opts = opts;
        let Some(w) = &self.wiring else {
            return false;
        };
        if !self.state.connected {
            return false;
        }
        (w.media)(true, opts);
        true
    }

    pub fn stream_options(&self) -> StreamOptions {
        self.opts
    }

    pub fn disconnect(&mut self) -> bool {
        if !self.state.connected {
            return false;
        }
        if let Some(w) = &self.wiring {
            (w.media)(false, self.opts);
        }
        self.router.record_stop();
        self.state.connected = false;
        self.state.controlling = false;
        self.emit();
        true
    }

    pub fn set_control(&mut self, enabled: bool) -> bool {
        if !self.state.connected {
            return false;
        }
        self.state.controlling = enabled;
        self.emit();
        true
    }

    pub fn input(&mut self, event: DesktopInputEvent) -> bool {
        if !self.state.connected || !self.state.controlling {
            return false;
        }
        let Some(w) = &self.wiring else {
            return false;
        };
        (w.send)(P2pMessage::RemoteDesktopInput {
            resource_id: w.resource_id.clone(),
            event,
        });
        true
    }

    pub fn unwire(&mut self) {
        self.disconnect();
        if self.wiring.is_none() && self.state.resource_id.is_none() {
            return;
        }
        self.wiring = None;
        self.state.resource_id = None;
        self.emit();
    }

    pub fn state(&self) -> &DesktopState {
        &self.state
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn emit(&mut self) {
        self.events.push(Event::DesktopChanged {
            desktop: self.state.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Sent = Rc<RefCell<Vec<P2pMessage>>>;
    type MediaLog = Rc<RefCell<Vec<bool>>>;

    fn rig() -> (Desktop, Sent, MediaLog) {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let media = Rc::new(RefCell::new(Vec::new()));
        let mut d = Desktop::new();
        let (s, m) = (sent.clone(), media.clone());
        d.set_resource(
            "screen-1",
            move |msg| s.borrow_mut().push(msg),
            move |on, _opts| m.borrow_mut().push(on),
        );
        d.take_events();
        (d, sent, media)
    }

    #[test]
    fn unwire_disconnects_and_drops_the_peer() {
        let (mut d, _, media) = rig();
        d.connect(StreamOptions::default());
        d.unwire();
        assert!(!d.state().connected);
        assert!(d.state().resource_id.is_none());
        assert_eq!(*media.borrow(), vec![true, false]);
        assert!(
            !d.connect(StreamOptions::default()),
            "an unwired desktop has nothing to connect to"
        );
    }

    fn mouse() -> DesktopInputEvent {
        DesktopInputEvent::MouseMove { x: 10, y: 20 }
    }

    #[test]
    fn screen_size_survives_frames_but_not_a_new_session() {
        let r = DesktopRouter::new();
        assert_eq!(r.screen_size(), None);

        r.set_screen_size(2560, 1440);
        r.record_frame(1280, 720, 10, &[0, 0, 0, 255]);
        assert_eq!(
            r.screen_size(),
            Some((2560, 1440)),
            "the stream is encoded smaller than the screen; the screen is what the pointer maps onto"
        );

        r.set_screen_size(0, 0);
        assert_eq!(
            r.screen_size(),
            Some((2560, 1440)),
            "a nil size is not an answer"
        );

        r.mark_started();
        assert_eq!(
            r.screen_size(),
            None,
            "the next session may be another host"
        );
    }

    #[test]
    fn connect_starts_media_and_a_fresh_measurement() {
        let (mut d, _, media) = rig();
        d.router().record_frame(1, 1, 1, &[0, 0, 0, 255]);
        assert!(d.connect(StreamOptions::default()));
        assert_eq!(*media.borrow(), vec![true]);
        assert!(d.state().connected);
        let st = d.router().stats();
        assert_eq!(st.metrics.frames_total, 0, "mark_started resets counters");
        assert!(d.router().latest_frame().is_none());
        assert!(matches!(
            d.take_events().last(),
            Some(Event::DesktopChanged { desktop }) if desktop.connected
        ));
    }

    #[test]
    fn input_is_dropped_unless_connected_and_controlling() {
        let (mut d, sent, _) = rig();
        assert!(!d.input(mouse()), "not connected → dropped");
        d.connect(StreamOptions::default());
        assert!(!d.input(mouse()), "connected but not controlling → dropped");
        assert!(sent.borrow().is_empty());

        assert!(d.set_control(true));
        assert!(d.input(mouse()));
        assert!(matches!(
            sent.borrow().last().unwrap(),
            P2pMessage::RemoteDesktopInput { resource_id, event: DesktopInputEvent::MouseMove { x: 10, y: 20 } }
                if resource_id == "screen-1"
        ));
    }

    #[test]
    fn disconnect_stops_media_counts_the_stop_and_revokes_control() {
        let (mut d, _, media) = rig();
        d.connect(StreamOptions::default());
        d.set_control(true);
        assert!(d.disconnect());
        assert_eq!(*media.borrow(), vec![true, false]);
        assert_eq!(d.router().stats().metrics.stream_stops, 1);
        assert!(
            !d.state().controlling,
            "control does not survive the session"
        );
        assert!(!d.set_control(true), "control needs a live session");
    }

    #[test]
    fn frames_account_into_per_session_metrics() {
        let (mut d, _, _) = rig();
        d.connect(StreamOptions::default());
        let r = d.router();
        r.record_frame(320, 180, 1000, &[1, 2, 3, 255]);
        r.record_frame(320, 180, 500, &[4, 5, 6, 255]);
        let st = r.stats();
        assert_eq!(st.metrics.frames_total, 2);
        assert_eq!(st.metrics.bytes_total, 1500);
        assert_eq!((st.metrics.last_width, st.metrics.last_height), (320, 180));
        assert!(st.time_to_first_frame_ms.is_some());
        assert!(st.ms_since_last_frame.is_some());
        let (w, h, bgra) = r.latest_frame().unwrap();
        assert_eq!((w, h), (320, 180));
        assert_eq!(bgra, vec![4, 5, 6, 255], "snapshot is the LATEST frame");
    }

    #[test]
    fn two_sessions_have_independent_metrics() {
        let a = DesktopRouter::new();
        let b = DesktopRouter::new();
        a.mark_started();
        b.mark_started();
        a.record_frame(10, 10, 100, &[0; 4]);
        assert_eq!(a.stats().metrics.frames_total, 1);
        assert_eq!(b.stats().metrics.frames_total, 0);
    }

    #[test]
    fn switching_resource_disconnects_the_old_session() {
        let (mut d, _, media) = rig();
        d.connect(StreamOptions::default());
        d.set_resource("screen-2", |_| {}, |_, _| {});
        assert!(!d.state().connected);
        assert_eq!(
            *media.borrow(),
            vec![true, false],
            "old media stopped via OLD wiring"
        );
        assert_eq!(d.state().resource_id.as_deref(), Some("screen-2"));
    }
}

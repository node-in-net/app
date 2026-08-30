use serde::{Deserialize, Serialize};
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum Stage {
    Welcome,
    NamingDevice { suggested_name: String },
    SigningIn,
    AllSet,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(flatten)]
    pub stage: Stage,
    pub node_id: Option<String>,
    pub device_name: Option<String>,
    pub account_login: Option<String>,
    pub guest: bool,
    pub device_registered: bool,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            stage: Stage::Welcome,
            node_id: None,
            device_name: None,
            account_login: None,
            guest: false,
            device_registered: false,
            last_error: None,
        }
    }
}

impl SessionState {
    pub fn restored(
        node_id: String,
        device_name: String,
        account_login: Option<String>,
        guest: bool,
    ) -> Self {
        Self {
            stage: Stage::Ready,
            node_id: Some(node_id),
            device_name: Some(device_name),
            account_login,
            guest,
            device_registered: true,
            last_error: None,
        }
    }

    pub fn begin_setup(&mut self, node_id: String, suggested_name: String) {
        self.node_id = Some(node_id);
        self.stage = Stage::NamingDevice { suggested_name };
    }

    pub fn confirm_device_name(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || !matches!(self.stage, Stage::NamingDevice { .. }) {
            return false;
        }
        self.device_name = Some(name.to_string());
        self.stage = Stage::SigningIn;
        true
    }

    pub fn signed_in(&mut self, account_login: String) {
        self.account_login = Some(account_login);
        self.guest = false;
        self.device_registered = true;
        self.stage = Stage::AllSet;
    }

    pub fn signed_in_as_guest(&mut self) {
        self.account_login = None;
        self.guest = true;
        self.device_registered = true;
        self.stage = Stage::AllSet;
    }

    pub fn enter_workspace(&mut self) {
        if self.stage == Stage::AllSet {
            self.stage = Stage::Ready;
        }
    }

    pub fn logout(&mut self) {
        self.account_login = None;
        self.device_name = None;
        self.guest = false;
        self.device_registered = false;
        self.stage = Stage::Welcome;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    SessionChanged { session: SessionState },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredSession {
    pub node_id: String,
    pub device_name: String,
    pub account_login: Option<String>,
}

#[async_trait::async_trait(?Send)]
pub trait AuthRpc {
    async fn login(&self, login: String, password: String) -> Result<(), String>;
    async fn register_device(&self, node_id: String, device_name: String) -> Result<(), String>;
    async fn join_temporary(&self) -> Result<(), String> {
        Err("this build cannot join without registering".into())
    }
    async fn restore(&self) -> Result<RestoredSession, String> {
        Err("no stored session".into())
    }
    async fn logout(&self) -> Result<(), String> {
        Ok(())
    }
    async fn set_turn_region(&self, _region: nodeinnet_p2p::TurnRegion) -> Result<(), String> {
        Err("not signed in".into())
    }
}

#[derive(Default)]
pub struct Session {
    rpc: Option<Rc<dyn AuthRpc>>,
    state: SessionState,
    events: Vec<Event>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn restored(state: SessionState) -> Self {
        Self {
            rpc: None,
            state,
            events: Vec::new(),
        }
    }

    pub async fn set_turn_region(
        &mut self,
        region: nodeinnet_p2p::TurnRegion,
    ) -> Result<(), String> {
        let Some(rpc) = self.rpc.clone() else {
            return Err("not signed in".into());
        };
        let res = rpc.set_turn_region(region).await;
        if let Err(e) = &res {
            self.state.last_error = Some(e.clone());
            self.emit();
        }
        res
    }

    pub fn set_rpc(&mut self, rpc: Rc<dyn AuthRpc>) {
        self.rpc = Some(rpc);
    }

    pub fn begin_setup(&mut self, node_id: String, suggested_name: String) {
        self.state.begin_setup(node_id, suggested_name);
        self.emit();
    }

    pub fn confirm_device_name(&mut self, name: &str) -> bool {
        let ok = self.state.confirm_device_name(name);
        if ok {
            self.emit();
        }
        ok
    }

    pub async fn sign_in(&mut self, login: String, password: String) -> bool {
        self.sign_in_as(login, password, false).await
    }

    pub async fn sign_in_as(&mut self, login: String, password: String, temporary: bool) -> bool {
        if self.state.stage != Stage::SigningIn {
            return false;
        }
        let (Some(rpc), Some(node_id), Some(device_name)) = (
            self.rpc.clone(),
            self.state.node_id.clone(),
            self.state.device_name.clone(),
        ) else {
            self.state.last_error = Some("auth is not wired".into());
            self.emit();
            return false;
        };
        let result = async {
            rpc.login(login.clone(), password).await?;
            if temporary {
                rpc.join_temporary().await
            } else {
                rpc.register_device(node_id, device_name).await
            }
        }
        .await;
        match result {
            Ok(()) => {
                self.state.last_error = None;
                self.state.signed_in(login);
                self.emit();
                true
            }
            Err(e) => {
                self.state.last_error = Some(e);
                self.emit();
                false
            }
        }
    }

    pub async fn restore(&mut self) -> bool {
        let Some(rpc) = self.rpc.clone() else {
            return false;
        };
        match rpc.restore().await {
            Ok(r) => {
                self.state =
                    SessionState::restored(r.node_id, r.device_name, r.account_login, false);
                self.emit();
                true
            }
            Err(_) => {
                self.emit();
                false
            }
        }
    }

    pub fn enter_workspace(&mut self) {
        if self.state.stage == Stage::AllSet {
            self.state.enter_workspace();
            self.emit();
        }
    }

    pub fn sign_in_as_guest(&mut self) -> bool {
        if self.state.stage != Stage::SigningIn {
            return false;
        }
        self.state.last_error = None;
        self.state.signed_in_as_guest();
        self.emit();
        true
    }

    pub fn logout(&mut self) {
        self.state.last_error = None;
        self.state.logout();
        self.emit();
    }

    pub async fn sign_out(&mut self) {
        if let Some(rpc) = self.rpc.clone() {
            let _ = rpc.logout().await;
        }
        self.state.last_error = None;
        self.state.logout();
        self.emit();
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn emit(&mut self) {
        self.events.push(Event::SessionChanged {
            session: self.state.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_walks_the_wizard() {
        let mut s = SessionState::default();
        assert_eq!(s.stage, Stage::Welcome);

        s.begin_setup("node-1".into(), "Igor's MacBook".into());
        assert!(
            matches!(s.stage, Stage::NamingDevice { ref suggested_name } if suggested_name == "Igor's MacBook")
        );

        assert!(s.confirm_device_name("  My Mac  "));
        assert_eq!(s.device_name.as_deref(), Some("My Mac"));
        assert_eq!(s.stage, Stage::SigningIn);

        s.signed_in("igor".into());
        assert_eq!(
            s.stage,
            Stage::AllSet,
            "success screen before the workspace"
        );
        assert!(s.device_registered);
        assert!(!s.guest);

        s.enter_workspace();
        assert_eq!(s.stage, Stage::Ready);
    }

    #[test]
    fn empty_device_name_is_rejected() {
        let mut s = SessionState::default();
        s.begin_setup("node-1".into(), "x".into());
        assert!(!s.confirm_device_name("   "));
        assert!(matches!(s.stage, Stage::NamingDevice { .. }));
    }

    #[test]
    fn confirm_name_outside_naming_stage_is_rejected() {
        let mut s = SessionState::default();
        assert!(!s.confirm_device_name("Mac"));
        assert_eq!(s.stage, Stage::Welcome);
    }

    #[test]
    fn guest_path_reaches_ready_without_account() {
        let mut s = SessionState::default();
        s.begin_setup("node-1".into(), "x".into());
        s.confirm_device_name("x");
        s.signed_in_as_guest();
        assert_eq!(s.stage, Stage::AllSet);
        s.enter_workspace();
        assert_eq!(s.stage, Stage::Ready);
        assert!(s.guest);
        assert!(s.account_login.is_none());
    }

    #[test]
    fn restored_session_skips_the_wizard() {
        let s = SessionState::restored("node-1".into(), "Mac".into(), Some("igor".into()), false);
        assert_eq!(s.stage, Stage::Ready);
    }

    #[test]
    fn logout_restarts_onboarding_keeping_the_crypto_identity() {
        let mut s =
            SessionState::restored("node-1".into(), "Mac".into(), Some("igor".into()), false);
        s.logout();
        assert_eq!(s.stage, Stage::Welcome, "full restart to page 1");
        assert_eq!(
            s.node_id.as_deref(),
            Some("node-1"),
            "device keypair survives"
        );
        assert!(
            s.device_name.is_none(),
            "name cleared → re-suggested on re-walk"
        );
        assert!(s.account_login.is_none());
        assert!(!s.device_registered);
    }

    use std::cell::RefCell;

    struct FakeAuth {
        fail_register: bool,
        registered: RefCell<Option<(String, String)>>,
        stored: Option<RestoredSession>,
        logged_out: std::cell::Cell<bool>,
        joined_temporary: std::cell::Cell<bool>,
    }

    #[async_trait::async_trait(?Send)]
    impl AuthRpc for FakeAuth {
        async fn login(&self, login: String, password: String) -> Result<(), String> {
            if login == "test" && password == "test" {
                Ok(())
            } else {
                Err("Invalid login or password".into())
            }
        }
        async fn register_device(
            &self,
            node_id: String,
            device_name: String,
        ) -> Result<(), String> {
            if self.fail_register {
                return Err("registration refused".into());
            }
            *self.registered.borrow_mut() = Some((node_id, device_name));
            Ok(())
        }
        async fn restore(&self) -> Result<RestoredSession, String> {
            self.stored
                .clone()
                .ok_or_else(|| "no stored session".into())
        }
        async fn logout(&self) -> Result<(), String> {
            self.logged_out.set(true);
            Ok(())
        }
        async fn join_temporary(&self) -> Result<(), String> {
            self.joined_temporary.set(true);
            Ok(())
        }
    }

    fn wizard_at_signin(fail_register: bool) -> (Session, Rc<FakeAuth>) {
        let rpc = Rc::new(FakeAuth {
            fail_register,
            registered: RefCell::new(None),
            stored: None,
            logged_out: std::cell::Cell::new(false),
            joined_temporary: std::cell::Cell::new(false),
        });
        let mut s = Session::new();
        s.set_rpc(rpc.clone());
        s.begin_setup("node-1".into(), "Igor's Mac".into());
        s.confirm_device_name("My Mac");
        s.take_events();
        (s, rpc)
    }

    #[tokio::test]
    async fn sign_in_logs_in_registers_and_reaches_ready() {
        let (mut s, rpc) = wizard_at_signin(false);
        assert!(s.sign_in("test".into(), "test".into()).await);
        assert_eq!(
            s.state().stage,
            Stage::AllSet,
            "lands on the success screen"
        );
        assert!(s.state().device_registered);
        assert_eq!(
            *rpc.registered.borrow(),
            Some(("node-1".into(), "My Mac".into())),
            "device registered under node_id with the confirmed name"
        );
        s.enter_workspace();
        assert_eq!(s.state().stage, Stage::Ready);
        assert!(matches!(
            s.take_events().last(),
            Some(Event::SessionChanged { session }) if session.stage == Stage::Ready
        ));
    }

    #[tokio::test]
    async fn bad_password_is_an_error_not_a_transition() {
        let (mut s, rpc) = wizard_at_signin(false);
        assert!(!s.sign_in("test".into(), "wrong".into()).await);
        assert_eq!(
            s.state().stage,
            Stage::SigningIn,
            "stays on the sign-in step"
        );
        assert!(s.state().last_error.as_deref().unwrap().contains("Invalid"));
        assert!(
            rpc.registered.borrow().is_none(),
            "registration never attempted"
        );
        assert!(s.sign_in("test".into(), "test".into()).await);
        assert!(s.state().last_error.is_none());
    }

    #[tokio::test]
    async fn failed_registration_blocks_ready() {
        let (mut s, _) = wizard_at_signin(true);
        assert!(!s.sign_in("test".into(), "test".into()).await);
        assert_eq!(s.state().stage, Stage::SigningIn);
        assert!(s.state().last_error.as_deref().unwrap().contains("refused"));
        assert!(!s.state().device_registered);
    }

    #[tokio::test]
    async fn sign_in_outside_the_signin_stage_is_rejected() {
        let mut s = Session::new();
        assert!(!s.sign_in("test".into(), "test".into()).await);
        assert_eq!(s.state().stage, Stage::Welcome);
    }

    #[tokio::test]
    async fn restore_with_a_stored_session_skips_the_wizard() {
        let rpc = Rc::new(FakeAuth {
            fail_register: false,
            registered: RefCell::new(None),
            stored: Some(RestoredSession {
                node_id: "node-1".into(),
                device_name: "My Mac".into(),
                account_login: Some("igor".into()),
            }),
            logged_out: std::cell::Cell::new(false),
            joined_temporary: std::cell::Cell::new(false),
        });
        let mut s = Session::new();
        s.set_rpc(rpc);
        assert!(s.restore().await);
        assert_eq!(s.state().stage, Stage::Ready);
        assert!(s.state().device_registered);
        assert_eq!(s.state().account_login.as_deref(), Some("igor"));
        assert_eq!(s.state().device_name.as_deref(), Some("My Mac"));
        assert!(matches!(
            s.take_events().last(),
            Some(Event::SessionChanged { session }) if session.stage == Stage::Ready
        ));
    }

    #[tokio::test]
    async fn restore_without_a_stored_session_falls_through_to_the_wizard() {
        let rpc = Rc::new(FakeAuth {
            fail_register: false,
            registered: RefCell::new(None),
            stored: None,
            logged_out: std::cell::Cell::new(false),
            joined_temporary: std::cell::Cell::new(false),
        });
        let mut s = Session::new();
        s.set_rpc(rpc);
        assert!(!s.restore().await, "nothing stored → restore fails");
        assert_eq!(s.state().stage, Stage::Welcome, "wizard still runs");
        assert!(s.state().last_error.is_none());
    }

    #[tokio::test]
    async fn sign_out_forgets_the_session_and_returns_to_signin() {
        let (mut s, rpc) = wizard_at_signin(false);
        assert!(s.sign_in("test".into(), "test".into()).await);
        s.enter_workspace();
        assert_eq!(s.state().stage, Stage::Ready);

        s.sign_out().await;
        assert_eq!(
            s.state().stage,
            Stage::Welcome,
            "sign-out restarts onboarding"
        );
        assert!(
            rpc.logged_out.get(),
            "driver was asked to forget the persisted session"
        );
        assert!(!s.state().device_registered);
        assert!(
            s.state().device_name.is_none(),
            "name cleared for the re-walk"
        );
        assert_eq!(
            s.state().node_id.as_deref(),
            Some("node-1"),
            "crypto identity survives"
        );
    }

    #[tokio::test]
    async fn guest_and_logout_round_trip_emits_events() {
        let (mut s, _) = wizard_at_signin(false);
        assert!(s.sign_in_as_guest());
        assert_eq!(s.state().stage, Stage::AllSet);
        assert!(s.state().guest);
        s.logout();
        assert_eq!(s.state().stage, Stage::Welcome);
        assert_eq!(s.take_events().len(), 2, "guest + logout each emitted");
    }

    #[tokio::test]
    async fn a_guest_signs_in_without_registering_a_device() {
        let (mut s, rpc) = wizard_at_signin(false);
        assert!(s.sign_in_as("test".into(), "test".into(), true).await);
        assert_eq!(s.state().stage, Stage::AllSet);
        assert!(
            rpc.joined_temporary.get(),
            "a guest must join the mesh, not merely skip registration"
        );
        assert!(
            rpc.registered.borrow().is_none(),
            "a guest that registers leaves exactly the row it was meant not to leave"
        );
    }

    #[tokio::test]
    async fn a_normal_sign_in_still_registers() {
        let (mut s, rpc) = wizard_at_signin(false);
        assert!(s.sign_in_as("test".into(), "test".into(), false).await);
        assert!(rpc.registered.borrow().is_some());
        assert!(!rpc.joined_temporary.get());
    }

    #[tokio::test]
    async fn a_failed_restore_reports_itself_so_the_ui_can_fall_back() {
        let rpc = Rc::new(FakeAuth {
            fail_register: false,
            registered: RefCell::new(None),
            stored: None,
            logged_out: std::cell::Cell::new(false),
            joined_temporary: std::cell::Cell::new(false),
        });
        let mut s = Session::new();
        s.set_rpc(rpc);
        s.take_events();

        assert!(!s.restore().await);
        assert!(
            !s.take_events().is_empty(),
            "a failure that emits nothing leaves the window on whatever it opened with"
        );
        assert_ne!(
            s.state().stage,
            Stage::Ready,
            "the emitted state must be one the shell renders as the wizard"
        );
    }
}

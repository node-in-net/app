use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const PORT_VAR: &str = "NODEINNET_NETWORK_PORT";

pub fn launch_proxied_app(exec_cmd: &str, socks_port: u16) -> std::io::Result<std::process::Child> {
    let override_lib = std::env::var("NODEINNET_NETWORK_LIB").ok();

    #[cfg(unix)]
    {
        #[cfg(debug_assertions)]
        let lib_path = if cfg!(target_os = "macos") {
            "libnode_network.dylib".to_string()
        } else {
            let local_debug = std::path::Path::new("target/debug/libnode_network.so");
            if local_debug.exists() {
                local_debug.to_string_lossy().to_string()
            } else {
                "/usr/lib/nodeinnet/libnode_network.so".to_string()
            }
        };

        #[cfg(not(debug_assertions))]
        let lib_path = {
            let exe_path =
                std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let exe_dir = exe_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            if cfg!(target_os = "macos") {
                let bundle_lib = exe_dir.join("../Libs/libnode_network.dylib");
                bundle_lib.to_string_lossy().to_string()
            } else {
                "/usr/lib/nodeinnet/libnode_network.so".to_string()
            }
        };

        let lib_path = override_lib.clone().unwrap_or(lib_path);

        let parts: Vec<&str> = exec_cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Err(std::io::Error::other("empty command line"));
        }

        let mut cmd = std::process::Command::new(parts[0]);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }

        cmd.env(PORT_VAR, socks_port.to_string());
        if cfg!(target_os = "macos") {
            cmd.env("DYLD_FORCE_FLAT_NAMESPACE", "1");
            cmd.env("DYLD_INSERT_LIBRARIES", lib_path);
        } else {
            cmd.env("LD_PRELOAD", lib_path);
        }

        cmd.spawn()
    }

    #[cfg(windows)]
    {
        #[cfg(debug_assertions)]
        let lib_path = "node_network.dll".to_string();

        #[cfg(not(debug_assertions))]
        let lib_path = {
            let exe_path =
                std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let exe_dir = exe_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            exe_dir
                .join("node_network.dll")
                .to_string_lossy()
                .into_owned()
        };

        let abs_lib_path = std::fs::canonicalize(&lib_path)
            .unwrap_or_else(|_| std::path::PathBuf::from(&lib_path));

        let path_obj = std::path::Path::new(exec_cmd);
        let mut cmd = if path_obj.exists() && path_obj.is_file() {
            std::process::Command::new(exec_cmd)
        } else {
            let parts: Vec<&str> = exec_cmd.split_whitespace().collect();
            if parts.is_empty() {
                return Err(std::io::Error::other("empty command line"));
            }
            let mut c = std::process::Command::new(parts[0]);
            if parts.len() > 1 {
                c.args(&parts[1..]);
            }
            c
        };

        cmd.env(PORT_VAR, socks_port.to_string());

        use std::os::windows::process::CommandExt;
        const CREATE_SUSPENDED: u32 = 0x00000004;

        let child = cmd.creation_flags(CREATE_SUSPENDED).spawn()?;

        let pid = child.id();
        let injection_result = std::panic::catch_unwind(|| unsafe {
            use std::os::windows::ffi::OsStrExt;
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
            use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
            use windows_sys::Win32::System::Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD,
                THREADENTRY32,
            };
            use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
            use windows_sys::Win32::System::Memory::{
                VirtualAllocEx, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE,
            };
            use windows_sys::Win32::System::Threading::{
                OpenThread, ResumeThread, TerminateProcess, WaitForSingleObject, INFINITE,
                THREAD_SUSPEND_RESUME,
            };

            extern "system" {
                fn CreateRemoteThread(
                    hProcess: isize,
                    lpThreadAttributes: *const std::ffi::c_void,
                    dwStackSize: usize,
                    lpStartAddress: *const std::ffi::c_void,
                    lpParameter: *const std::ffi::c_void,
                    dwCreationFlags: u32,
                    lpThreadId: *mut u32,
                ) -> isize;
            }

            let h_process = child.as_raw_handle() as isize;
            let path_w16: Vec<u16> = abs_lib_path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let size = path_w16.len() * 2;

            let addr = VirtualAllocEx(
                h_process,
                std::ptr::null(),
                size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            );

            let mut injected = false;
            if !addr.is_null() {
                WriteProcessMemory(
                    h_process,
                    addr,
                    path_w16.as_ptr() as *const std::ffi::c_void,
                    size,
                    std::ptr::null_mut(),
                );

                let h_kernel32 = GetModuleHandleA(b"kernel32.dll\0".as_ptr());
                let p_load_lib = GetProcAddress(h_kernel32, b"LoadLibraryW\0".as_ptr());

                let h_thread = CreateRemoteThread(
                    h_process,
                    std::ptr::null(),
                    0,
                    std::mem::transmute(p_load_lib),
                    addr,
                    0,
                    std::ptr::null_mut(),
                );

                if h_thread != 0 {
                    WaitForSingleObject(h_thread, INFINITE);
                    injected = true;
                }
            }

            if !injected {
                TerminateProcess(h_process, 1);
                return Err("Failed to inject tracking DLL into the application process. Check antivirus or permissions.");
            }

            let h_snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if h_snap != INVALID_HANDLE_VALUE {
                let mut te32: THREADENTRY32 = std::mem::zeroed();
                te32.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;

                if Thread32First(h_snap, &mut te32) != 0 {
                    loop {
                        if te32.th32OwnerProcessID == pid {
                            let h_thread = OpenThread(THREAD_SUSPEND_RESUME, 0, te32.th32ThreadID);
                            if h_thread != 0 {
                                ResumeThread(h_thread);
                                CloseHandle(h_thread);
                            }
                        }
                        if Thread32Next(h_snap, &mut te32) == 0 {
                            break;
                        }
                    }
                }
                CloseHandle(h_snap);
            }
            Ok(())
        });

        match injection_result {
            Ok(Ok(())) => Ok(child),
            Ok(Err(msg)) => Err(std::io::Error::other(msg)),
            Err(_) => Err(std::io::Error::other("DLL injection panicked")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LaunchedApp {
    pub session_id: uuid::Uuid,
    pub pid: u32,
    pub peer_id: String,
    pub command: String,
    pub app_id: Option<String>,
    pub remote: bool,
    pub started_unix_ms: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct TunnelTotals {
    pub apps: usize,
    pub peers: usize,
    pub sockets: usize,
    pub bytes_up: u64,
    pub bytes_down: u64,
}

struct Tunnel {
    proxy: crate::SocksProxy,
    router: Arc<app_core::webvpn::WebVpnRouter>,
    children: Vec<(std::process::Child, LaunchedApp)>,
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Default)]
pub struct SocksManager {
    tunnels: Arc<Mutex<HashMap<String, Tunnel>>>,
}

impl SocksManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn launch(
        &self,
        peer_id: &str,
        resource_id: &str,
        exec_cmd: &str,
        net_tx: tokio::sync::mpsc::Sender<client_core::NetCmd>,
        router: Arc<app_core::webvpn::WebVpnRouter>,
    ) -> std::io::Result<u32> {
        self.launch_session(peer_id, resource_id, exec_cmd, net_tx, router, None, None)
            .map(|app| app.pid)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn launch_session(
        &self,
        peer_id: &str,
        resource_id: &str,
        exec_cmd: &str,
        net_tx: tokio::sync::mpsc::Sender<client_core::NetCmd>,
        router: Arc<app_core::webvpn::WebVpnRouter>,
        session_id: Option<uuid::Uuid>,
        app_id: Option<String>,
    ) -> std::io::Result<LaunchedApp> {
        let mut tunnels = self.tunnels.lock().unwrap();
        if let Some(id) = session_id {
            if let Some(existing) = tunnels
                .values()
                .flat_map(|t| t.children.iter().map(|(_, a)| a))
                .find(|a| a.session_id == id)
            {
                return Ok(existing.clone());
            }
        }
        let tunnel = match tunnels.entry(peer_id.to_string()) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let proxy = crate::SocksProxy::start(
                    resource_id.to_string(),
                    peer_id.to_string(),
                    net_tx,
                    router.clone(),
                )?;
                e.insert(Tunnel {
                    proxy,
                    router,
                    children: Vec::new(),
                })
            }
        };
        let child = launch_proxied_app(exec_cmd, tunnel.proxy.port())?;
        let app = LaunchedApp {
            session_id: session_id.unwrap_or_else(uuid::Uuid::new_v4),
            pid: child.id(),
            peer_id: peer_id.to_string(),
            command: exec_cmd.to_string(),
            remote: app_id.is_some(),
            app_id,
            started_unix_ms: now_unix_ms(),
        };
        tunnel.children.push((child, app.clone()));
        Ok(app)
    }

    pub fn sessions_for(&self, peer_id: &str) -> Vec<nodeinnet_p2p::p2p::RemoteAppSession> {
        let tunnels = self.tunnels.lock().unwrap();
        tunnels
            .get(peer_id)
            .map(|t| {
                t.children
                    .iter()
                    .filter(|(_, a)| a.remote)
                    .map(|(_, a)| nodeinnet_p2p::p2p::RemoteAppSession {
                        session_id: a.session_id,
                        app_id: a.app_id.clone().unwrap_or_default(),
                        started_unix_ms: a.started_unix_ms,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn terminate_session(&self, peer_id: &str, session_id: uuid::Uuid) -> bool {
        let mut tunnels = self.tunnels.lock().unwrap();
        let Some(tunnel) = tunnels.get_mut(peer_id) else {
            return false;
        };
        match tunnel
            .children
            .iter_mut()
            .find(|(_, a)| a.session_id == session_id)
        {
            Some((child, _)) => child.kill().is_ok(),
            None => false,
        }
    }

    pub fn sessions_of_app(&self, app_id: &str) -> Vec<LaunchedApp> {
        let tunnels = self.tunnels.lock().unwrap();
        tunnels
            .values()
            .flat_map(|t| t.children.iter().map(|(_, a)| a))
            .filter(|a| a.app_id.as_deref() == Some(app_id))
            .cloned()
            .collect()
    }

    pub fn reap(&self) -> Vec<LaunchedApp> {
        let mut exited = Vec::new();
        let mut tunnels = self.tunnels.lock().unwrap();
        tunnels.retain(|_peer, tunnel| {
            tunnel
                .children
                .retain_mut(|(child, app)| match child.try_wait() {
                    Ok(Some(_)) => {
                        exited.push(app.clone());
                        false
                    }
                    Ok(None) => true,
                    Err(_) => {
                        exited.push(app.clone());
                        false
                    }
                });
            !tunnel.children.is_empty()
        });
        exited
    }

    pub fn terminate(&self, pid: u32) -> bool {
        let mut tunnels = self.tunnels.lock().unwrap();
        for tunnel in tunnels.values_mut() {
            if let Some((child, _)) = tunnel.children.iter_mut().find(|(_, a)| a.pid == pid) {
                return child.kill().is_ok();
            }
        }
        false
    }

    pub fn close_all(&self) {
        let mut tunnels = self.tunnels.lock().unwrap();
        for (_peer, tunnel) in tunnels.iter_mut() {
            for (child, _) in tunnel.children.iter_mut() {
                let _ = child.kill();
            }
        }
        tunnels.clear();
    }

    pub fn close_peer(&self, peer_id: &str) {
        if let Some(mut tunnel) = self.tunnels.lock().unwrap().remove(peer_id) {
            for (child, _) in tunnel.children.iter_mut() {
                let _ = child.kill();
            }
        }
    }

    pub fn totals(&self) -> TunnelTotals {
        let tunnels = self.tunnels.lock().unwrap();
        let mut totals = TunnelTotals {
            peers: tunnels.len(),
            ..Default::default()
        };
        let mut counted: Vec<*const app_core::webvpn::WebVpnRouter> = Vec::new();
        for tunnel in tunnels.values() {
            totals.apps += tunnel.children.len();
            let ptr = Arc::as_ptr(&tunnel.router);
            if counted.contains(&ptr) {
                continue;
            }
            counted.push(ptr);
            let t = tunnel.router.totals();
            totals.sockets += t.streams_opened.saturating_sub(t.streams_closed) as usize;
            totals.bytes_up += t.bytes_up;
            totals.bytes_down += t.bytes_down;
        }
        totals
    }

    pub fn apps(&self) -> Vec<LaunchedApp> {
        let tunnels = self.tunnels.lock().unwrap();
        let mut apps: Vec<LaunchedApp> = tunnels
            .values()
            .flat_map(|t| t.children.iter().map(|(_, a)| a.clone()))
            .collect();
        apps.sort_by_key(|a| a.started_unix_ms);
        apps
    }

    pub fn port_for(&self, peer_id: &str) -> Option<u16> {
        self.tunnels
            .lock()
            .unwrap()
            .get(peer_id)
            .map(|t| t.proxy.port())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::webvpn::WebVpnRouter;

    fn manager_with_one_app(cmd: &str) -> (SocksManager, u16, u32) {
        let mgr = SocksManager::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let pid = mgr
            .launch("peer-a", "res-a", cmd, tx, WebVpnRouter::new())
            .expect("launch");
        let port = mgr.port_for("peer-a").expect("tunnel is open");
        (mgr, port, pid)
    }

    #[test]
    fn a_tunnel_opens_per_peer_not_per_app() {
        let mgr = SocksManager::new();
        let (tx_a, _ra) = tokio::sync::mpsc::channel(1);
        let (tx_b, _rb) = tokio::sync::mpsc::channel(1);
        mgr.launch(
            "peer-a",
            "res-a",
            "sleep 30",
            tx_a.clone(),
            WebVpnRouter::new(),
        )
        .expect("first app");
        mgr.launch("peer-a", "res-a", "sleep 30", tx_a, WebVpnRouter::new())
            .expect("second app, same peer");
        mgr.launch("peer-b", "res-b", "sleep 30", tx_b, WebVpnRouter::new())
            .expect("app on another peer");

        let totals = mgr.totals();
        assert_eq!(totals.apps, 3);
        assert_eq!(totals.peers, 2, "two peers, two listeners");
        assert_ne!(
            mgr.port_for("peer-a"),
            mgr.port_for("peer-b"),
            "each peer redirects to its own port"
        );

        mgr.close_peer("peer-a");
        mgr.close_peer("peer-b");
    }

    #[test]
    fn the_child_is_told_which_port_to_use() {
        let (mgr, port, _) = manager_with_one_app("sh -c exit");
        assert!(port >= 54145);
        assert_eq!(
            std::env::var(PORT_VAR).ok(),
            None,
            "set on the child, not on us"
        );
        drop(mgr);
    }

    #[test]
    fn the_last_app_to_exit_closes_the_tunnel() {
        let (mgr, port, _) = manager_with_one_app("sh -c exit");

        let mut closed = false;
        for _ in 0..200 {
            mgr.reap();
            if mgr.port_for("peer-a").is_none() {
                closed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(closed, "tunnel outlived its only app");
        assert_eq!(mgr.totals().apps, 0);

        let mut freed = false;
        for _ in 0..40 {
            if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
                freed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(freed, "port {port} stayed bound after the app exited");
    }

    fn spawn_exit(
        mut rx: tokio::sync::mpsc::Receiver<client_core::NetCmd>,
        router: Arc<WebVpnRouter>,
    ) {
        use nodeinnet_p2p::P2pMessage as M;
        tokio::spawn(async move {
            let mut writers: HashMap<uuid::Uuid, tokio::sync::mpsc::Sender<Vec<u8>>> =
                HashMap::new();
            while let Some(client_core::NetCmd::SendToPeer(_peer, msg)) = rx.recv().await {
                match msg {
                    M::SocksConnectRequest {
                        resource_id,
                        stream_id,
                        host,
                        port,
                    } => {
                        let Ok(sock) = tokio::net::TcpStream::connect((host.as_str(), port)).await
                        else {
                            router.route(M::SocksConnectResponse {
                                resource_id,
                                stream_id,
                                is_success: false,
                                error_msg: Some("refused".into()),
                            });
                            continue;
                        };
                        router.route(M::SocksConnectResponse {
                            resource_id: resource_id.clone(),
                            stream_id,
                            is_success: true,
                            error_msg: None,
                        });

                        let (mut rd, mut wr) = sock.into_split();
                        let (tx, mut out) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
                        writers.insert(stream_id, tx);

                        tokio::spawn(async move {
                            use tokio::io::AsyncWriteExt;
                            while let Some(chunk) = out.recv().await {
                                if wr.write_all(&chunk).await.is_err() {
                                    break;
                                }
                            }
                        });
                        let router = router.clone();
                        tokio::spawn(async move {
                            use tokio::io::AsyncReadExt;
                            let mut buf = vec![0u8; 4096];
                            loop {
                                match rd.read(&mut buf).await {
                                    Ok(0) | Err(_) => break,
                                    Ok(n) => router.route(M::SocksData {
                                        resource_id: resource_id.clone(),
                                        stream_id,
                                        data: buf[..n].to_vec(),
                                    }),
                                };
                            }
                            router.route(M::SocksClose {
                                resource_id,
                                stream_id,
                            });
                        });
                    }
                    M::SocksData {
                        stream_id, data, ..
                    } => {
                        if let Some(tx) = writers.get(&stream_id) {
                            let _ = tx.send(data).await;
                        }
                    }
                    M::SocksClose { stream_id, .. } => {
                        writers.remove(&stream_id);
                    }
                    _ => {}
                }
            }
        });
    }

    async fn one_shot_server(body: &'static str) -> u16 {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut b = [0u8; 1024];
                    let _ = s.read(&mut b).await;
                    let _ = s
                        .write_all(
                            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body)
                                .as_bytes(),
                        )
                        .await;
                    let _ = s.shutdown().await;
                });
            }
        });
        port
    }

    async fn fetch_through(port: u16, target: u16) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut c = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("the listener was bound before launch returned");
        c.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut sel = [0u8; 2];
        c.read_exact(&mut sel).await.unwrap();
        let mut req = vec![0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1];
        req.extend_from_slice(&target.to_be_bytes());
        c.write_all(&req).await.unwrap();
        let mut reply = [0u8; 10];
        c.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[1], 0x00, "the exit refused the connection");
        c.write_all(b"GET / HTTP/1.0\r\nHost: t\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut got = Vec::new();
        let _ =
            tokio::time::timeout(std::time::Duration::from_secs(10), c.read_to_end(&mut got)).await;
        String::from_utf8_lossy(&got).to_string()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn two_peers_use_each_other_at_once_without_crossing_streams() {
        let via_b = one_shot_server("reached through B").await;
        let via_a = one_shot_server("reached through A").await;

        let router_a = WebVpnRouter::new();
        let router_b = WebVpnRouter::new();
        let (tx_a, rx_a) = tokio::sync::mpsc::channel(64);
        let (tx_b, rx_b) = tokio::sync::mpsc::channel(64);
        spawn_exit(rx_a, router_a.clone());
        spawn_exit(rx_b, router_b.clone());

        let mgr_a = SocksManager::new();
        let mgr_b = SocksManager::new();
        mgr_a
            .launch("peer-b", "b-net", "sleep 60", tx_a, router_a)
            .expect("A opens a tunnel toward B");
        mgr_b
            .launch("peer-a", "a-net", "sleep 60", tx_b, router_b)
            .expect("B opens a tunnel toward A");

        let port_a = mgr_a.port_for("peer-b").expect("A's listener");
        let port_b = mgr_b.port_for("peer-a").expect("B's listener");
        assert_ne!(port_a, port_b, "one listener per peer, never a shared port");

        let (from_b, from_a) =
            tokio::join!(fetch_through(port_a, via_b), fetch_through(port_b, via_a));

        assert!(
            from_b.contains("reached through B"),
            "A's traffic did not leave through B: {from_b}"
        );
        assert!(
            from_a.contains("reached through A"),
            "B's traffic did not leave through A: {from_a}"
        );

        mgr_a.close_all();
        mgr_b.close_all();
    }

    #[test]
    fn one_shared_router_is_not_counted_once_per_peer() {
        let router = WebVpnRouter::new();
        let mgr = SocksManager::new();
        let (tx_a, _ra) = tokio::sync::mpsc::channel(1);
        let (tx_b, _rb) = tokio::sync::mpsc::channel(1);
        mgr.launch("peer-a", "res-a", "sleep 30", tx_a, router.clone())
            .unwrap();
        mgr.launch("peer-b", "res-b", "sleep 30", tx_b, router.clone())
            .unwrap();

        let before = mgr.totals();
        assert_eq!(before.peers, 2);
        assert_eq!(before.sockets, 0, "nothing is open yet");

        let id = uuid::Uuid::new_v4();
        let _rx = router.register_stream(id);
        let after = mgr.totals();
        assert_eq!(
            after.sockets, 1,
            "one open stream is one socket, however many peers share the router"
        );
        mgr.close_all();
    }

    #[test]
    fn a_repeated_request_adopts_the_running_session() {
        let mgr = SocksManager::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let id = uuid::Uuid::new_v4();
        let first = mgr
            .launch_session(
                "peer-a",
                "res",
                "sleep 30",
                tx.clone(),
                WebVpnRouter::new(),
                Some(id),
                Some("firefox".into()),
            )
            .expect("first launch");
        let second = mgr
            .launch_session(
                "peer-a",
                "res",
                "sleep 30",
                tx,
                WebVpnRouter::new(),
                Some(id),
                Some("firefox".into()),
            )
            .expect("retry");

        assert_eq!(first.pid, second.pid, "no second copy on someone's desktop");
        assert_eq!(mgr.totals().apps, 1);
        mgr.close_all();
    }

    #[test]
    fn one_device_cannot_stop_anothers_session() {
        let mgr = SocksManager::new();
        let (tx_a, _ra) = tokio::sync::mpsc::channel(1);
        let (tx_b, _rb) = tokio::sync::mpsc::channel(1);
        let id = uuid::Uuid::new_v4();
        mgr.launch_session(
            "peer-a",
            "res-a",
            "sleep 30",
            tx_a,
            WebVpnRouter::new(),
            Some(id),
            Some("firefox".into()),
        )
        .unwrap();
        mgr.launch_session(
            "peer-b",
            "res-b",
            "sleep 30",
            tx_b,
            WebVpnRouter::new(),
            Some(uuid::Uuid::new_v4()),
            Some("firefox".into()),
        )
        .unwrap();

        assert!(
            !mgr.terminate_session("peer-b", id),
            "a sibling device must not reach another device's session"
        );
        assert!(mgr.terminate_session("peer-a", id), "its owner may stop it");
        mgr.close_all();
    }

    #[test]
    fn only_remote_sessions_are_offered_to_the_peer() {
        let mgr = SocksManager::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        mgr.launch("peer-a", "res", "sleep 30", tx.clone(), WebVpnRouter::new())
            .unwrap();
        assert!(mgr.sessions_for("peer-a").is_empty());

        mgr.launch_session(
            "peer-a",
            "res",
            "sleep 30",
            tx,
            WebVpnRouter::new(),
            Some(uuid::Uuid::new_v4()),
            Some("firefox".into()),
        )
        .unwrap();
        assert_eq!(mgr.sessions_for("peer-a").len(), 1);
        assert_eq!(mgr.sessions_of_app("firefox").len(), 1);
        mgr.close_all();
    }

    #[test]
    fn terminating_the_last_app_takes_its_tunnel_with_it() {
        let mgr = SocksManager::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let pid = mgr
            .launch("peer-a", "res-a", "sleep 30", tx, WebVpnRouter::new())
            .expect("launch");
        assert!(mgr.apps().iter().any(|a| a.pid == pid));

        assert!(
            mgr.terminate(pid),
            "the pid we were handed is the one we hold"
        );
        assert!(
            !mgr.terminate(pid + 100_000),
            "unknown pid is refused, not panicked on"
        );

        let mut gone = false;
        for _ in 0..200 {
            mgr.reap();
            if mgr.port_for("peer-a").is_none() {
                gone = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(gone, "terminated app left its tunnel open");
        assert!(mgr.apps().is_empty());
    }

    #[test]
    fn closing_a_peer_leaves_the_other_alone() {
        let mgr = SocksManager::new();
        let (tx_a, _ra) = tokio::sync::mpsc::channel(1);
        let (tx_b, _rb) = tokio::sync::mpsc::channel(1);
        mgr.launch("peer-a", "res-a", "sleep 30", tx_a, WebVpnRouter::new())
            .unwrap();
        mgr.launch("peer-b", "res-b", "sleep 30", tx_b, WebVpnRouter::new())
            .unwrap();

        mgr.close_peer("peer-a");
        assert!(mgr.port_for("peer-a").is_none());
        assert!(mgr.port_for("peer-b").is_some(), "peer-b was untouched");
        assert_eq!(mgr.totals().peers, 1);

        mgr.close_peer("peer-b");
    }
}

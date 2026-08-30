#![allow(unsafe_op_in_unsafe_fn)]

use lazy_static::lazy_static;
use std::ffi::c_void;
use std::mem;
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{BOOL, CloseHandle, HANDLE, HINSTANCE, HMODULE};
use windows_sys::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, FIONBIO, LPWSAOVERLAPPED_COMPLETION_ROUTINE, SO_TYPE, SOCK_DGRAM, SOCKADDR,
    SOCKADDR_IN, SOCKADDR_IN6, SOCKET, SOCKET_ERROR, SOL_SOCKET, WSABUF, WSAEACCES,
    WSAGetLastError, WSASetLastError, getsockopt, ioctlsocket, recv, send,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows_sys::Win32::System::IO::OVERLAPPED;
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleA, GetProcAddress,
};
use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx};
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CreateRemoteThread, PROCESS_INFORMATION, ResumeThread, STARTUPINFOA,
    STARTUPINFOW, WaitForSingleObject,
};
use windows_sys::core::{PSTR, PWSTR};

use minhook::MinHook;

type ConnectFunc = unsafe extern "system" fn(SOCKET, *const SOCKADDR, i32) -> i32;

type IoctlsocketFunc = unsafe extern "system" fn(SOCKET, i32, *mut u32) -> i32;
type ClosesocketFunc = unsafe extern "system" fn(SOCKET) -> i32;

type SendtoFunc =
    unsafe extern "system" fn(SOCKET, *const u8, i32, i32, *const SOCKADDR, i32) -> i32;

type WsaSendToFunc = unsafe extern "system" fn(
    SOCKET,
    *const WSABUF,
    u32,
    *mut u32,
    u32,
    *const SOCKADDR,
    i32,
    *mut OVERLAPPED,
    LPWSAOVERLAPPED_COMPLETION_ROUTINE,
) -> i32;

type CreateProcessWFunc = unsafe extern "system" fn(
    PWSTR,
    PWSTR,
    *const SECURITY_ATTRIBUTES,
    *const SECURITY_ATTRIBUTES,
    BOOL,
    u32,
    *const c_void,
    PWSTR,
    *const STARTUPINFOW,
    *mut PROCESS_INFORMATION,
) -> BOOL;

type CreateProcessAFunc = unsafe extern "system" fn(
    PSTR,
    PSTR,
    *const SECURITY_ATTRIBUTES,
    *const SECURITY_ATTRIBUTES,
    BOOL,
    u32,
    *const c_void,
    PSTR,
    *const STARTUPINFOA,
    *mut PROCESS_INFORMATION,
) -> BOOL;

lazy_static! {
    static ref TRAMPOLINE_CONNECT: Mutex<Option<ConnectFunc>> = Mutex::new(None);
    static ref TRAMPOLINE_IOCTLSOCKET: Mutex<Option<IoctlsocketFunc>> = Mutex::new(None);
    static ref TRAMPOLINE_CLOSESOCKET: Mutex<Option<ClosesocketFunc>> = Mutex::new(None);
    static ref APP_NONBLOCKING: Mutex<std::collections::HashMap<SOCKET, u32>> =
        Mutex::new(std::collections::HashMap::new());
    static ref TRAMPOLINE_SENDTO: Mutex<Option<SendtoFunc>> = Mutex::new(None);
    static ref TRAMPOLINE_WSASENDTO: Mutex<Option<WsaSendToFunc>> = Mutex::new(None);
    static ref TRAMPOLINE_CP_W: Mutex<Option<CreateProcessWFunc>> = Mutex::new(None);
    static ref TRAMPOLINE_CP_A: Mutex<Option<CreateProcessAFunc>> = Mutex::new(None);
    static ref DLL_PATH: Mutex<Vec<u16>> = Mutex::new(Vec::new());
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _reserved: *mut c_void) -> i32 {
    match reason {
        DLL_PROCESS_ATTACH => {
            unsafe {
                let mut path_buf = [0u16; 1024];
                let len = GetModuleFileNameW(hinst as HMODULE, path_buf.as_mut_ptr(), 1024);
                if len > 0 {
                    let mut path_vec = path_buf[..len as usize].to_vec();
                    path_vec.push(0);
                    *DLL_PATH.lock().unwrap() = path_vec;
                }
            }
            initialize_windows_hooks();
        }
        DLL_PROCESS_DETACH => unsafe {
            let _ = MinHook::disable_all_hooks();
        },
        _ => {}
    }
    1
}

pub fn initialize_windows_hooks() {
    unsafe {
        let tramp_connect =
            MinHook::create_hook_api("ws2_32.dll", "connect", my_connect as *mut std::ffi::c_void);
        if let Ok(orig) = tramp_connect {
            *TRAMPOLINE_CONNECT.lock().unwrap() = Some(mem::transmute(orig));
        }

        let tramp_ioctl = MinHook::create_hook_api(
            "ws2_32.dll",
            "ioctlsocket",
            my_ioctlsocket as *mut std::ffi::c_void,
        );
        if let Ok(orig) = tramp_ioctl {
            *TRAMPOLINE_IOCTLSOCKET.lock().unwrap() = Some(mem::transmute(orig));
        }

        let tramp_close = MinHook::create_hook_api(
            "ws2_32.dll",
            "closesocket",
            my_closesocket as *mut std::ffi::c_void,
        );
        if let Ok(orig) = tramp_close {
            *TRAMPOLINE_CLOSESOCKET.lock().unwrap() = Some(mem::transmute(orig));
        }

        let tramp_sendto =
            MinHook::create_hook_api("ws2_32.dll", "sendto", my_sendto as *mut std::ffi::c_void);
        if let Ok(orig) = tramp_sendto {
            *TRAMPOLINE_SENDTO.lock().unwrap() = Some(mem::transmute(orig));
        }

        let tramp_wsasendto = MinHook::create_hook_api(
            "ws2_32.dll",
            "WSASendTo",
            my_WSASendTo as *mut std::ffi::c_void,
        );
        if let Ok(orig) = tramp_wsasendto {
            *TRAMPOLINE_WSASENDTO.lock().unwrap() = Some(mem::transmute(orig));
        }

        let tramp_cpw = MinHook::create_hook_api(
            "kernel32.dll",
            "CreateProcessW",
            my_CreateProcessW as *mut std::ffi::c_void,
        );
        if let Ok(orig) = tramp_cpw {
            *TRAMPOLINE_CP_W.lock().unwrap() = Some(mem::transmute(orig));
        }

        let tramp_cpa = MinHook::create_hook_api(
            "kernel32.dll",
            "CreateProcessA",
            my_CreateProcessA as *mut std::ffi::c_void,
        );
        if let Ok(orig) = tramp_cpa {
            *TRAMPOLINE_CP_A.lock().unwrap() = Some(mem::transmute(orig));
        }

        let _ = MinHook::enable_all_hooks();
    }
}

unsafe fn inject_dll(h_process: HANDLE) {
    let path_w16 = DLL_PATH.lock().unwrap().clone();
    if path_w16.is_empty() {
        return;
    }

    let size = path_w16.len() * 2;
    let addr = VirtualAllocEx(
        h_process,
        std::ptr::null(),
        size,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    );

    if !addr.is_null() {
        let mut written = 0;
        let res = WriteProcessMemory(
            h_process,
            addr,
            path_w16.as_ptr() as *const std::ffi::c_void,
            size,
            &mut written,
        );

        if res != 0 {
            let kernel32 = std::ffi::CString::new("kernel32.dll").unwrap();
            let load_lib = std::ffi::CString::new("LoadLibraryW").unwrap();
            let h_kernel32 = GetModuleHandleA(kernel32.as_ptr() as *const u8);
            if h_kernel32 != 0 {
                let load_lib_addr = GetProcAddress(h_kernel32, load_lib.as_ptr() as *const u8);
                if let Some(load_lib_fn) = load_lib_addr {
                    let mut tid = 0;

                    let h_thread = CreateRemoteThread(
                        h_process,
                        std::ptr::null(),
                        0,
                        Some(mem::transmute(load_lib_fn)),
                        addr,
                        0,
                        &mut tid,
                    );

                    if h_thread != 0 {
                        WaitForSingleObject(h_thread, 10000);
                        CloseHandle(h_thread);
                    }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
pub unsafe extern "system" fn my_CreateProcessW(
    lpApplicationName: PWSTR,
    lpCommandLine: PWSTR,
    lpProcessAttributes: *const SECURITY_ATTRIBUTES,
    lpThreadAttributes: *const SECURITY_ATTRIBUTES,
    bInheritHandles: BOOL,
    dwCreationFlags: u32,
    lpEnvironment: *const c_void,
    lpCurrentDirectory: PWSTR,
    lpStartupInfo: *const STARTUPINFOW,
    lpProcessInformation: *mut PROCESS_INFORMATION,
) -> BOOL {
    let orig = {
        let lock = TRAMPOLINE_CP_W.lock().unwrap();
        lock.unwrap()
    };

    let mut skip_injection = false;
    let is_conhost = |s: PWSTR| -> bool {
        if s.is_null() {
            return false;
        }
        let mut ptr = s;
        let mut buf = Vec::new();
        while *ptr != 0 {
            buf.push(*ptr);
            ptr = ptr.add(1);
        }
        if let Ok(cmd) = String::from_utf16(&buf) {
            let lower = cmd.to_lowercase();
            lower.contains("conhost.exe") || lower.contains("openconsole.exe")
        } else {
            false
        }
    };
    if is_conhost(lpApplicationName) || is_conhost(lpCommandLine) {
        skip_injection = true;
    }

    let was_suspended = (dwCreationFlags & CREATE_SUSPENDED) != 0;

    let new_flags = if skip_injection {
        dwCreationFlags
    } else {
        dwCreationFlags | CREATE_SUSPENDED
    };

    let res = orig(
        lpApplicationName,
        lpCommandLine,
        lpProcessAttributes,
        lpThreadAttributes,
        bInheritHandles,
        new_flags,
        lpEnvironment,
        lpCurrentDirectory,
        lpStartupInfo,
        lpProcessInformation,
    );

    if res != 0 {
        if !skip_injection {
            inject_dll((*lpProcessInformation).hProcess);
            if !was_suspended {
                ResumeThread((*lpProcessInformation).hThread);
            }
        }
    }

    res
}

#[allow(non_snake_case)]
pub unsafe extern "system" fn my_CreateProcessA(
    lpApplicationName: PSTR,
    lpCommandLine: PSTR,
    lpProcessAttributes: *const SECURITY_ATTRIBUTES,
    lpThreadAttributes: *const SECURITY_ATTRIBUTES,
    bInheritHandles: BOOL,
    dwCreationFlags: u32,
    lpEnvironment: *const c_void,
    lpCurrentDirectory: PSTR,
    lpStartupInfo: *const STARTUPINFOA,
    lpProcessInformation: *mut PROCESS_INFORMATION,
) -> BOOL {
    let orig = {
        let lock = TRAMPOLINE_CP_A.lock().unwrap();
        lock.unwrap()
    };

    let mut skip_injection = false;
    let is_conhost = |s: PSTR| -> bool {
        if s.is_null() {
            return false;
        }
        let cstr = std::ffi::CStr::from_ptr(s as *const i8);
        if let Ok(cmd) = cstr.to_str() {
            let lower = cmd.to_lowercase();
            lower.contains("conhost.exe") || lower.contains("openconsole.exe")
        } else {
            false
        }
    };
    if is_conhost(lpApplicationName) || is_conhost(lpCommandLine) {
        skip_injection = true;
    }

    let was_suspended = (dwCreationFlags & CREATE_SUSPENDED) != 0;

    let new_flags = if skip_injection {
        dwCreationFlags
    } else {
        dwCreationFlags | CREATE_SUSPENDED
    };

    let res = orig(
        lpApplicationName,
        lpCommandLine,
        lpProcessAttributes,
        lpThreadAttributes,
        bInheritHandles,
        new_flags,
        lpEnvironment,
        lpCurrentDirectory,
        lpStartupInfo,
        lpProcessInformation,
    );

    if res != 0 {
        if !skip_injection {
            inject_dll((*lpProcessInformation).hProcess);
            if !was_suspended {
                ResumeThread((*lpProcessInformation).hThread);
            }
        }
    }

    res
}

pub unsafe extern "system" fn my_ioctlsocket(s: SOCKET, cmd: i32, argp: *mut u32) -> i32 {
    if cmd == FIONBIO && !argp.is_null() {
        APP_NONBLOCKING.lock().unwrap().insert(s, *argp);
    }
    let orig = { TRAMPOLINE_IOCTLSOCKET.lock().unwrap().unwrap() };
    orig(s, cmd, argp)
}

pub unsafe extern "system" fn my_closesocket(s: SOCKET) -> i32 {
    APP_NONBLOCKING.lock().unwrap().remove(&s);
    let orig = { TRAMPOLINE_CLOSESOCKET.lock().unwrap().unwrap() };
    orig(s)
}

unsafe fn set_nonblocking_unhooked(s: SOCKET, mut mode: u32) {
    let orig = { *TRAMPOLINE_IOCTLSOCKET.lock().unwrap() };
    match orig {
        Some(f) => {
            f(s, FIONBIO, &mut mode);
        }
        None => {
            ioctlsocket(s, FIONBIO, &mut mode);
        }
    }
}

unsafe fn restore_app_blocking_mode(s: SOCKET) {
    let mode = APP_NONBLOCKING
        .lock()
        .unwrap()
        .get(&s)
        .copied()
        .unwrap_or(0);
    if mode != 0 {
        set_nonblocking_unhooked(s, mode);
    }
}

unsafe fn is_external_udp(s: SOCKET, to: *const SOCKADDR) -> bool {
    if to.is_null() {
        return false;
    }

    let sa_family = (*to).sa_family;
    if sa_family != AF_INET && sa_family != AF_INET6 {
        return false;
    }

    let mut sock_type: i32 = 0;
    let mut opt_len: i32 = mem::size_of::<i32>() as i32;
    if getsockopt(
        s,
        SOL_SOCKET,
        SO_TYPE,
        &mut sock_type as *mut i32 as *mut u8,
        &mut opt_len,
    ) != 0
        || sock_type != SOCK_DGRAM
    {
        return false;
    }

    if sa_family == AF_INET {
        let addr_in = &*(to as *const SOCKADDR_IN);
        let ip_bytes = addr_in.sin_addr.S_un.S_addr.to_ne_bytes();
        if ip_bytes[0] == 127 {
            return false;
        }
    } else {
        let addr_in6 = &*(to as *const SOCKADDR_IN6);
        if addr_in6.sin6_addr.u.Byte == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1] {
            return false;
        }
    }

    true
}

unsafe fn refuse_datagram() -> i32 {
    WSASetLastError(WSAEACCES);
    SOCKET_ERROR
}

pub unsafe extern "system" fn my_sendto(
    s: SOCKET,
    buf: *const u8,
    len: i32,
    flags: i32,
    to: *const SOCKADDR,
    tolen: i32,
) -> i32 {
    if is_external_udp(s, to) {
        return refuse_datagram();
    }
    let orig = { TRAMPOLINE_SENDTO.lock().unwrap().unwrap() };
    orig(s, buf, len, flags, to, tolen)
}

#[allow(non_snake_case)]
pub unsafe extern "system" fn my_WSASendTo(
    s: SOCKET,
    lpbuffers: *const WSABUF,
    dwbuffercount: u32,
    lpnumberofbytessent: *mut u32,
    dwflags: u32,
    lpto: *const SOCKADDR,
    itolen: i32,
    lpoverlapped: *mut OVERLAPPED,
    lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE,
) -> i32 {
    if is_external_udp(s, lpto) {
        return refuse_datagram();
    }
    let orig = { TRAMPOLINE_WSASENDTO.lock().unwrap().unwrap() };
    orig(
        s,
        lpbuffers,
        dwbuffercount,
        lpnumberofbytessent,
        dwflags,
        lpto,
        itolen,
        lpoverlapped,
        lpcompletionroutine,
    )
}

pub unsafe extern "system" fn my_connect(s: SOCKET, name: *const SOCKADDR, namelen: i32) -> i32 {
    let orig_connect = {
        let lock = TRAMPOLINE_CONNECT.lock().unwrap();
        lock.unwrap()
    };

    if name.is_null() {
        return orig_connect(s, name, namelen);
    }

    let sa_family = (*name).sa_family as u16;
    if sa_family != AF_INET && sa_family != windows_sys::Win32::Networking::WinSock::AF_INET6 {
        return orig_connect(s, name, namelen);
    }

    let mut is_ipv6 = false;
    let mut ip_bytes_v4: [u8; 4] = [0; 4];
    let mut ip_bytes_v6: [u8; 16] = [0; 16];
    let port_bytes: [u8; 2];

    if sa_family == AF_INET {
        let addr_in = &*(name as *const SOCKADDR_IN);
        let ip_u32 = addr_in.sin_addr.S_un.S_addr;
        ip_bytes_v4 = ip_u32.to_ne_bytes();
        port_bytes = addr_in.sin_port.to_ne_bytes();

        if ip_bytes_v4[0] == 127 {
            return orig_connect(s, name, namelen);
        }
    } else {
        is_ipv6 = true;
        let addr_in6 = &*(name as *const windows_sys::Win32::Networking::WinSock::SOCKADDR_IN6);
        ip_bytes_v6 = addr_in6.sin6_addr.u.Byte;
        port_bytes = addr_in6.sin6_port.to_ne_bytes();

        if ip_bytes_v6 == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1] {
            return orig_connect(s, name, namelen);
        }
    }

    let mut sock_type: i32 = 0;
    let mut opt_len: i32 = mem::size_of::<i32>() as i32;
    if getsockopt(
        s,
        SOL_SOCKET,
        SO_TYPE,
        &mut sock_type as *mut i32 as *mut u8,
        &mut opt_len,
    ) == 0
        && sock_type == SOCK_DGRAM
    {
        return refuse_datagram();
    }

    set_nonblocking_unhooked(s, 0);
    let outcome = socks_handshake(
        s,
        orig_connect,
        is_ipv6,
        ip_bytes_v4,
        ip_bytes_v6,
        port_bytes,
    );
    let err = WSAGetLastError();
    restore_app_blocking_mode(s);
    WSASetLastError(err);
    outcome
}

unsafe fn socks_handshake(
    s: SOCKET,
    orig_connect: ConnectFunc,
    is_ipv6: bool,
    ip_bytes_v4: [u8; 4],
    ip_bytes_v6: [u8; 16],
    port_bytes: [u8; 2],
) -> i32 {
    let res = if !is_ipv6 {
        let mut proxy_addr: SOCKADDR_IN = mem::zeroed();
        proxy_addr.sin_family = AF_INET as u16;
        proxy_addr.sin_port = 1080u16.to_be();
        proxy_addr.sin_addr.S_un.S_addr = u32::from_ne_bytes([127, 0, 0, 1]);
        orig_connect(
            s,
            &proxy_addr as *const _ as *const SOCKADDR,
            mem::size_of_val(&proxy_addr) as i32,
        )
    } else {
        let mut proxy_addr6: windows_sys::Win32::Networking::WinSock::SOCKADDR_IN6 = mem::zeroed();
        proxy_addr6.sin6_family = windows_sys::Win32::Networking::WinSock::AF_INET6 as u16;
        proxy_addr6.sin6_port = 1080u16.to_be();
        proxy_addr6.sin6_addr.u.Byte = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        orig_connect(
            s,
            &proxy_addr6 as *const _ as *const SOCKADDR,
            mem::size_of_val(&proxy_addr6) as i32,
        )
    };

    if res != 0 {
        return res;
    }

    let greeting = [0x05u8, 0x01, 0x00];
    if send(s, greeting.as_ptr() as *const u8, greeting.len() as i32, 0) < 0 {
        return -1;
    }

    let mut resp1 = [0u8; 2];
    let n = recv(s, resp1.as_mut_ptr() as *mut u8, resp1.len() as i32, 0);
    if n != 2 || resp1[0] != 0x05 || resp1[1] != 0x00 {
        return -1;
    }

    let mut req = vec![0x05u8, 0x01, 0x00];
    if !is_ipv6 {
        req.push(0x01);
        req.extend_from_slice(&ip_bytes_v4);
    } else {
        req.push(0x04);
        req.extend_from_slice(&ip_bytes_v6);
    }
    req.extend_from_slice(&port_bytes);

    if send(s, req.as_ptr() as *const u8, req.len() as i32, 0) < 0 {
        return -1;
    }

    let mut resp2 = [0u8; 10];
    let n2 = recv(s, resp2.as_mut_ptr() as *mut u8, resp2.len() as i32, 0);

    if n2 < 10 || resp2[1] != 0x00 {
        windows_sys::Win32::Networking::WinSock::WSASetLastError(
            windows_sys::Win32::Networking::WinSock::WSAECONNREFUSED,
        );
        return -1;
    }

    0
}

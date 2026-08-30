#![allow(unsafe_op_in_unsafe_fn)]

#[cfg(not(target_os = "macos"))]
use lazy_static::lazy_static;
use libc::{AF_INET, c_int, sockaddr, sockaddr_in, sockaddr_in6, socklen_t};
use std::mem;
#[cfg(not(target_os = "macos"))]
use std::sync::Mutex;
use std::sync::OnceLock;

type ConnectFunc = unsafe extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int;
type SendtoFunc = unsafe extern "C" fn(
    c_int,
    *const libc::c_void,
    libc::size_t,
    c_int,
    *const sockaddr,
    socklen_t,
) -> libc::ssize_t;
type SendmsgFunc = unsafe extern "C" fn(c_int, *const libc::msghdr, c_int) -> libc::ssize_t;
#[cfg(not(target_os = "macos"))]
type SendmmsgFunc = unsafe extern "C" fn(c_int, *mut libc::mmsghdr, libc::c_uint, c_int) -> c_int;

#[cfg(not(target_os = "macos"))]
lazy_static! {
    static ref ORIGINAL_CONNECT: Mutex<Option<ConnectFunc>> = Mutex::new(None);
    static ref ORIGINAL_SENDTO: Mutex<Option<SendtoFunc>> = Mutex::new(None);
    static ref ORIGINAL_SENDMSG: Mutex<Option<SendmsgFunc>> = Mutex::new(None);
    static ref ORIGINAL_SENDMMSG: Mutex<Option<SendmmsgFunc>> = Mutex::new(None);
}

#[cfg(not(target_os = "macos"))]
fn get_original_connect() -> ConnectFunc {
    let mut conn_lock = ORIGINAL_CONNECT.lock().unwrap();
    if let Some(func) = *conn_lock {
        return func;
    }

    unsafe {
        let sym_name = c"connect";
        let func_ptr = libc::dlsym(libc::RTLD_NEXT, sym_name.as_ptr());

        if func_ptr.is_null() {
            panic!("NodeNetwork: Could not find original connect function");
        }

        let func: ConnectFunc = mem::transmute(func_ptr);
        *conn_lock = Some(func);
        func
    }
}

#[cfg(not(target_os = "macos"))]
fn get_original_sendto() -> SendtoFunc {
    let mut lock = ORIGINAL_SENDTO.lock().unwrap();
    if let Some(func) = *lock {
        return func;
    }
    unsafe {
        let sym_name = c"sendto";
        let func_ptr = libc::dlsym(libc::RTLD_NEXT, sym_name.as_ptr());
        if func_ptr.is_null() {
            panic!("NodeNetwork: Could not find original sendto");
        }
        let func: SendtoFunc = mem::transmute(func_ptr);
        *lock = Some(func);
        func
    }
}

#[cfg(not(target_os = "macos"))]
fn get_original_sendmsg() -> SendmsgFunc {
    let mut lock = ORIGINAL_SENDMSG.lock().unwrap();
    if let Some(func) = *lock {
        return func;
    }
    unsafe {
        let sym_name = c"sendmsg";
        let func_ptr = libc::dlsym(libc::RTLD_NEXT, sym_name.as_ptr());
        if func_ptr.is_null() {
            panic!("NodeNetwork: Could not find original sendmsg");
        }
        let func: SendmsgFunc = mem::transmute(func_ptr);
        *lock = Some(func);
        func
    }
}

#[cfg(not(target_os = "macos"))]
fn get_original_sendmmsg() -> SendmmsgFunc {
    let mut lock = ORIGINAL_SENDMMSG.lock().unwrap();
    if let Some(func) = *lock {
        return func;
    }
    unsafe {
        let sym_name = c"sendmmsg";
        let func_ptr = libc::dlsym(libc::RTLD_NEXT, sym_name.as_ptr());
        if func_ptr.is_null() {
            panic!("NodeNetwork: Could not find original sendmmsg");
        }
        let func: SendmmsgFunc = mem::transmute(func_ptr);
        *lock = Some(func);
        func
    }
}

#[cfg(target_os = "macos")]
#[repr(C)]
pub struct Interpose {
    pub replacement: *const (),
    pub original: *const (),
}

#[cfg(target_os = "macos")]
unsafe impl Sync for Interpose {}

#[cfg(target_os = "macos")]
#[used]
#[unsafe(link_section = "__DATA,__interpose")]
pub static INTERPOSE_CONNECT: Interpose = Interpose {
    replacement: my_connect as *const (),
    original: libc::connect as *const (),
};

#[cfg(target_os = "macos")]
#[used]
#[unsafe(link_section = "__DATA,__interpose")]
pub static INTERPOSE_SENDTO: Interpose = Interpose {
    replacement: my_sendto as *const (),
    original: libc::sendto as *const (),
};

#[cfg(target_os = "macos")]
#[used]
#[unsafe(link_section = "__DATA,__interpose")]
pub static INTERPOSE_SENDMSG: Interpose = Interpose {
    replacement: my_sendmsg as *const (),
    original: libc::sendmsg as *const (),
};

#[cfg(not(target_os = "macos"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn connect(
    sockfd: c_int,
    addr: *const sockaddr,
    addrlen: socklen_t,
) -> c_int {
    my_connect(sockfd, addr, addrlen)
}

#[cfg(not(target_os = "macos"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sendto(
    sockfd: c_int,
    buf: *const libc::c_void,
    len: libc::size_t,
    flags: c_int,
    dest_addr: *const sockaddr,
    addrlen: socklen_t,
) -> libc::ssize_t {
    my_sendto(sockfd, buf, len, flags, dest_addr, addrlen)
}

#[cfg(not(target_os = "macos"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sendmsg(
    sockfd: c_int,
    msg: *const libc::msghdr,
    flags: c_int,
) -> libc::ssize_t {
    my_sendmsg(sockfd, msg, flags)
}

#[cfg(not(target_os = "macos"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sendmmsg(
    sockfd: c_int,
    msgvec: *mut libc::mmsghdr,
    vlen: libc::c_uint,
    flags: c_int,
) -> c_int {
    my_sendmmsg(sockfd, msgvec, vlen, flags)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_connect(
    sockfd: c_int,
    addr: *const sockaddr,
    addrlen: socklen_t,
) -> c_int {
    #[cfg(target_os = "macos")]
    let orig_connect: ConnectFunc = libc::connect;

    #[cfg(not(target_os = "macos"))]
    let orig_connect = get_original_connect();

    if addr.is_null() {
        return orig_connect(sockfd, addr, addrlen);
    }

    let sa_family = (*addr).sa_family as c_int;
    if sa_family != AF_INET && sa_family != libc::AF_INET6 {
        return orig_connect(sockfd, addr, addrlen);
    }

    let is_ipv6 = sa_family == libc::AF_INET6;
    let mut ip_bytes = Vec::new();
    let port_bytes: [u8; 2];

    if !is_ipv6 {
        let addr_in = &*(addr as *const sockaddr_in);
        ip_bytes.extend_from_slice(&addr_in.sin_addr.s_addr.to_ne_bytes());
        port_bytes = addr_in.sin_port.to_ne_bytes();

        if ip_bytes[0] == 127 {
            return orig_connect(sockfd, addr, addrlen);
        }
    } else {
        let addr_in6 = &*(addr as *const libc::sockaddr_in6);
        ip_bytes.extend_from_slice(&addr_in6.sin6_addr.s6_addr);
        port_bytes = addr_in6.sin6_port.to_ne_bytes();

        if ip_bytes == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1] {
            return orig_connect(sockfd, addr, addrlen);
        }
    }

    let mut sock_type: c_int = 0;
    let mut opt_len: socklen_t = mem::size_of::<c_int>() as socklen_t;
    if libc::getsockopt(
        sockfd,
        libc::SOL_SOCKET,
        libc::SO_TYPE,
        &mut sock_type as *mut _ as *mut libc::c_void,
        &mut opt_len,
    ) == 0
        && sock_type == libc::SOCK_DGRAM
    {
        set_errno(libc::EACCES);
        return -1;
    }

    let flags = libc::fcntl(sockfd, libc::F_GETFL, 0);
    let was_nonblock = (flags & libc::O_NONBLOCK) != 0;
    if was_nonblock {
        libc::fcntl(sockfd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
    }

    let port = proxy_port();
    if port == 0 {
        if was_nonblock {
            libc::fcntl(sockfd, libc::F_SETFL, flags);
        }
        set_errno(libc::ECONNREFUSED);
        return -1;
    }

    let res = if !is_ipv6 {
        let mut proxy_addr: sockaddr_in = mem::zeroed();
        proxy_addr.sin_family = libc::AF_INET as libc::sa_family_t;
        proxy_addr.sin_port = port.to_be();
        proxy_addr.sin_addr.s_addr = u32::from_ne_bytes([127, 0, 0, 1]);
        orig_connect(
            sockfd,
            &proxy_addr as *const _ as *const sockaddr,
            mem::size_of_val(&proxy_addr) as socklen_t,
        )
    } else {
        let mut proxy_addr6: libc::sockaddr_in6 = mem::zeroed();
        proxy_addr6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
        proxy_addr6.sin6_port = port.to_be();
        proxy_addr6.sin6_addr.s6_addr = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        orig_connect(
            sockfd,
            &proxy_addr6 as *const _ as *const sockaddr,
            mem::size_of_val(&proxy_addr6) as socklen_t,
        )
    };

    if res < 0 {
        if was_nonblock {
            libc::fcntl(sockfd, libc::F_SETFL, flags);
        }
        return res;
    }

    let greeting = [0x05u8, 0x01, 0x00];
    if libc::send(
        sockfd,
        greeting.as_ptr() as *const libc::c_void,
        greeting.len(),
        0,
    ) < 0
    {
        if was_nonblock {
            libc::fcntl(sockfd, libc::F_SETFL, flags);
        }
        return -1;
    }

    let mut resp1 = [0u8; 2];
    let n = libc::recv(
        sockfd,
        resp1.as_mut_ptr() as *mut libc::c_void,
        resp1.len(),
        0,
    );
    if n != 2 {
        if was_nonblock {
            libc::fcntl(sockfd, libc::F_SETFL, flags);
        }
        return -1;
    }

    if resp1[0] != 0x05 || resp1[1] != 0x00 {
        if was_nonblock {
            libc::fcntl(sockfd, libc::F_SETFL, flags);
        }
        return -1;
    }

    let mut req = vec![0x05u8, 0x01, 0x00];
    if !is_ipv6 {
        req.push(0x01);
    } else {
        req.push(0x04);
    }
    req.extend_from_slice(&ip_bytes);
    req.extend_from_slice(&port_bytes);

    let len1 = libc::send(sockfd, req.as_ptr() as *const libc::c_void, req.len(), 0);
    if len1 < 0 {
        if was_nonblock {
            libc::fcntl(sockfd, libc::F_SETFL, flags);
        }
        return -1;
    }

    let mut resp_header = [0u8; 4];
    let n2 = libc::recv(sockfd, resp_header.as_mut_ptr() as *mut libc::c_void, 4, 0);

    let mut is_success = false;
    if n2 == 4 && resp_header[1] == 0x00 {
        is_success = true;
        let read_len = match resp_header[3] {
            0x01 => 6,
            0x04 => 18,
            0x03 => {
                let mut len_byte = [0u8; 1];
                libc::recv(sockfd, len_byte.as_mut_ptr() as *mut libc::c_void, 1, 0);
                len_byte[0] as usize + 2
            }
            _ => 0,
        };

        if read_len > 0 {
            let mut buf = vec![0u8; read_len];
            libc::recv(sockfd, buf.as_mut_ptr() as *mut libc::c_void, read_len, 0);
        }
    }

    if was_nonblock {
        libc::fcntl(sockfd, libc::F_SETFL, flags);
    }

    if !is_success {
        set_errno(libc::ECONNREFUSED);
        return -1;
    }

    0
}

unsafe fn set_errno(code: c_int) {
    #[cfg(target_os = "macos")]
    unsafe {
        unsafe extern "C" {
            fn __error() -> *mut libc::c_int;
        }
        *__error() = code;
    }
    #[cfg(not(target_os = "macos"))]
    unsafe {
        unsafe extern "C" {
            fn __errno_location() -> *mut libc::c_int;
        }
        *__errno_location() = code;
    }
}

fn proxy_port() -> u16 {
    static PORT: OnceLock<u16> = OnceLock::new();
    *PORT.get_or_init(|| {
        std::env::var("NODEINNET_NETWORK_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(0)
    })
}

unsafe fn is_external_udp(sockfd: c_int, dest_addr: *const sockaddr) -> bool {
    let sa_family = (*dest_addr).sa_family as c_int;
    if sa_family != AF_INET && sa_family != libc::AF_INET6 {
        return false;
    }

    let mut sock_type: c_int = 0;
    let mut opt_len: socklen_t = mem::size_of::<c_int>() as socklen_t;
    if libc::getsockopt(
        sockfd,
        libc::SOL_SOCKET,
        libc::SO_TYPE,
        &mut sock_type as *mut _ as *mut libc::c_void,
        &mut opt_len,
    ) != 0
        || sock_type != libc::SOCK_DGRAM
    {
        return false;
    }

    if sa_family == AF_INET {
        let addr_in = &*(dest_addr as *const sockaddr_in);
        let ip_bytes: [u8; 4] = addr_in.sin_addr.s_addr.to_ne_bytes();
        if ip_bytes[0] == 127 {
            return false;
        }
    } else if sa_family == libc::AF_INET6 {
        let addr_in6 = &*(dest_addr as *const sockaddr_in6);
        let ip_bytes = addr_in6.sin6_addr.s6_addr;
        if ip_bytes == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1] {
            return false;
        }
    }

    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_sendto(
    sockfd: c_int,
    buf: *const libc::c_void,
    len: libc::size_t,
    flags: c_int,
    dest_addr: *const sockaddr,
    addrlen: socklen_t,
) -> libc::ssize_t {
    #[cfg(target_os = "macos")]
    let orig: SendtoFunc = libc::sendto;

    #[cfg(not(target_os = "macos"))]
    let orig = get_original_sendto();

    if dest_addr.is_null() {
        return orig(sockfd, buf, len, flags, dest_addr, addrlen);
    }

    if is_external_udp(sockfd, dest_addr) {
        set_errno(libc::EACCES);
        return -1;
    }

    orig(sockfd, buf, len, flags, dest_addr, addrlen)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_sendmsg(
    sockfd: c_int,
    msg: *const libc::msghdr,
    flags: c_int,
) -> libc::ssize_t {
    #[cfg(target_os = "macos")]
    let orig: SendmsgFunc = libc::sendmsg;

    #[cfg(not(target_os = "macos"))]
    let orig = get_original_sendmsg();

    if msg.is_null() || (*msg).msg_name.is_null() {
        return orig(sockfd, msg, flags);
    }

    let dest_addr = (*msg).msg_name as *const sockaddr;
    if is_external_udp(sockfd, dest_addr) {
        set_errno(libc::EACCES);
        return -1;
    }

    orig(sockfd, msg, flags)
}

#[cfg(not(target_os = "macos"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_sendmmsg(
    sockfd: c_int,
    msgvec: *mut libc::mmsghdr,
    vlen: libc::c_uint,
    flags: c_int,
) -> c_int {
    let orig = get_original_sendmmsg();

    if msgvec.is_null() || vlen == 0 {
        return orig(sockfd, msgvec, vlen, flags);
    }

    let first_msg_name = (*msgvec).msg_hdr.msg_name;
    if first_msg_name.is_null() {
        return orig(sockfd, msgvec, vlen, flags);
    }

    let dest_addr = first_msg_name as *const sockaddr;
    if is_external_udp(sockfd, dest_addr) {
        set_errno(libc::EACCES);
        return -1;
    }

    orig(sockfd, msgvec, vlen, flags)
}

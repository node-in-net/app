use std::collections::{BTreeSet, HashMap};
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

const POOL: u32 = u32::from_be_bytes([198, 18, 0, 0]);
const POOL_MASK: u32 = !((1 << 17) - 1);
const FIRST: u32 = 2;
const LAST: u32 = (1 << 17) - 2;
pub const RESOLVER: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 1);
const TTL: u32 = 60;
/// Apps keep answers past their TTL, so an address is reused only this long after its last answer.
const REUSE_AFTER: Duration = Duration::from_secs(600);

const TYPE_A: u16 = 1;
const CLASS_IN: u16 = 1;
const SERVFAIL: u16 = 2;
const NXDOMAIN: u16 = 3;
const NOTIMP: u16 = 4;

pub struct FakeDns {
    by_name: HashMap<String, Ipv4Addr>,
    by_addr: HashMap<Ipv4Addr, (String, Instant)>,
    by_age: BTreeSet<(Instant, Ipv4Addr)>,
    next: u32,
}

impl Default for FakeDns {
    fn default() -> Self {
        Self {
            by_name: HashMap::new(),
            by_addr: HashMap::new(),
            by_age: BTreeSet::new(),
            next: FIRST,
        }
    }
}

impl FakeDns {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_fake(addr: Ipv4Addr) -> bool {
        u32::from(addr) & POOL_MASK == POOL
    }

    pub fn name_of(&self, addr: Ipv4Addr) -> Option<&str> {
        self.by_addr.get(&addr).map(|(name, _)| name.as_str())
    }

    pub fn address_of(&mut self, name: &str) -> Option<Ipv4Addr> {
        self.address_at(name, Instant::now())
    }

    fn address_at(&mut self, name: &str, now: Instant) -> Option<Ipv4Addr> {
        if let Some(addr) = self.by_name.get(name).copied() {
            if let Some((_, handed)) = self.by_addr.get_mut(&addr) {
                self.by_age.remove(&(*handed, addr));
                *handed = now;
                self.by_age.insert((now, addr));
            }
            return Some(addr);
        }
        let addr = if self.next <= LAST {
            self.next += 1;
            Ipv4Addr::from(POOL | (self.next - 1))
        } else {
            let (handed, addr) = *self.by_age.first()?;
            if now.duration_since(handed) < REUSE_AFTER {
                return None;
            }
            self.by_age.remove(&(handed, addr));
            if let Some((old, _)) = self.by_addr.remove(&addr) {
                self.by_name.remove(&old);
            }
            addr
        };
        self.by_addr.insert(addr, (name.to_string(), now));
        self.by_age.insert((now, addr));
        self.by_name.insert(name.to_string(), addr);
        Some(addr)
    }

    /// Every type but `A` gets an empty answer, so that apps fall back to IPv4 and keep the name.
    pub fn answer(&mut self, query: &[u8]) -> Option<Vec<u8>> {
        self.answer_at(query, Instant::now())
    }

    fn answer_at(&mut self, query: &[u8], now: Instant) -> Option<Vec<u8>> {
        let header = query.get(..12)?;
        let flags = u16::from_be_bytes([header[2], header[3]]);
        if flags & 0x8000 != 0 {
            return None;
        }
        let questions = u16::from_be_bytes([header[4], header[5]]);
        if flags & 0x7800 != 0 || questions != 1 {
            return Some(reply(header, flags, NOTIMP, &[], None));
        }
        let (name, end) = read_name(query, 12)?;
        let fields = query.get(end..end + 4)?;
        let qtype = u16::from_be_bytes([fields[0], fields[1]]);
        let qclass = u16::from_be_bytes([fields[2], fields[3]]);
        let question = &query[12..end + 4];
        let Some(name) = name else {
            return Some(reply(header, flags, NXDOMAIN, question, None));
        };
        if qtype != TYPE_A || qclass != CLASS_IN || name.is_empty() {
            return Some(reply(header, flags, 0, question, None));
        }
        Some(match self.address_at(&name, now) {
            Some(addr) => reply(header, flags, 0, question, Some(addr)),
            None => reply(header, flags, SERVFAIL, question, None),
        })
    }
}

fn read_name(packet: &[u8], mut pos: usize) -> Option<(Option<String>, usize)> {
    let mut name = String::new();
    let mut valid = true;
    loop {
        let len = *packet.get(pos)? as usize;
        pos += 1;
        if len == 0 {
            break;
        }
        if len > 63 {
            return None;
        }
        let label = packet.get(pos..pos + len)?;
        pos += len;
        valid &= label
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_');
        if !name.is_empty() {
            name.push('.');
        }
        name.extend(label.iter().map(|b| b.to_ascii_lowercase() as char));
    }
    valid &= name.len() <= 253;
    Some((valid.then_some(name), pos))
}

fn reply(
    header: &[u8],
    flags: u16,
    rcode: u16,
    question: &[u8],
    addr: Option<Ipv4Addr>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + question.len() + 16);
    out.extend_from_slice(&header[..2]);
    let flags = 0x8000 | (flags & 0x7900) | 0x0080 | rcode;
    out.extend_from_slice(&flags.to_be_bytes());
    for count in [
        u16::from(!question.is_empty()),
        u16::from(addr.is_some()),
        0,
        0,
    ] {
        out.extend_from_slice(&count.to_be_bytes());
    }
    out.extend_from_slice(question);
    if let Some(addr) = addr {
        out.extend_from_slice(&[0xC0, 0x0C]);
        out.extend_from_slice(&TYPE_A.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        out.extend_from_slice(&TTL.to_be_bytes());
        out.extend_from_slice(&4u16.to_be_bytes());
        out.extend_from_slice(&addr.octets());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPE_AAAA: u16 = 28;
    const TYPE_HTTPS: u16 = 65;

    pub(crate) fn query(id: u16, name: &str, qtype: u16) -> Vec<u8> {
        let mut q = id.to_be_bytes().to_vec();
        q.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
        for label in name.split('.') {
            q.push(label.len() as u8);
            q.extend_from_slice(label.as_bytes());
        }
        q.push(0);
        q.extend_from_slice(&qtype.to_be_bytes());
        q.extend_from_slice(&CLASS_IN.to_be_bytes());
        q
    }

    #[test]
    fn an_a_query_gets_one_pool_address_with_the_question_echoed() {
        let mut dns = FakeDns::new();
        let q = query(0x1234, "example.com", TYPE_A);
        let r = dns.answer(&q).expect("a reply");

        let question = &q[12..];
        let mut expected = vec![0x12, 0x34, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
        expected.extend_from_slice(question);
        expected.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 198, 18, 0, 2]);
        assert_eq!(r, expected);
        assert_eq!(
            dns.name_of(Ipv4Addr::new(198, 18, 0, 2)),
            Some("example.com")
        );
    }

    #[test]
    fn a_name_keeps_its_address_whatever_its_case() {
        let mut dns = FakeDns::new();
        let a = dns.answer(&query(1, "Example.COM", TYPE_A)).unwrap();
        let b = dns.answer(&query(2, "example.com", TYPE_A)).unwrap();
        let other = dns.answer(&query(3, "example.org", TYPE_A)).unwrap();
        assert_eq!(a[a.len() - 4..], b[b.len() - 4..]);
        assert_ne!(a[a.len() - 4..], other[other.len() - 4..]);
    }

    #[test]
    fn aaaa_and_https_get_an_empty_answer_so_apps_fall_back_to_ipv4() {
        let mut dns = FakeDns::new();
        for qtype in [TYPE_AAAA, TYPE_HTTPS] {
            let q = query(7, "example.com", qtype);
            let r = dns.answer(&q).unwrap();
            assert_eq!(r[2..4], [0x81, 0x80], "NOERROR");
            assert_eq!(r[6..8], [0, 0], "no answer for type {qtype}");
            assert_eq!(r.len(), q.len());
        }
        assert_eq!(dns.by_name.len(), 0, "nothing allocated for them");
    }

    #[test]
    fn a_name_that_is_not_a_host_gets_nxdomain() {
        let mut dns = FakeDns::new();
        let r = dns.answer(&query(9, "bad/name.example", TYPE_A)).unwrap();
        assert_eq!(r[3] & 0x0F, NXDOMAIN as u8);
        assert_eq!(r[6..8], [0, 0]);
    }

    #[test]
    fn what_is_not_a_plain_query_is_dropped_or_refused() {
        let mut dns = FakeDns::new();
        let mut response = query(1, "example.com", TYPE_A);
        response[2] |= 0x80;
        assert!(
            dns.answer(&response).is_none(),
            "a response is not answered"
        );

        let q = query(1, "example.com", TYPE_A);
        assert!(dns.answer(&q[..q.len() - 2]).is_none(), "cut short");

        let mut pointer = q[..12].to_vec();
        pointer.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1]);
        assert!(dns.answer(&pointer).is_none(), "a compressed question");

        let mut update = q.clone();
        update[2] = 0x28;
        assert_eq!(dns.answer(&update).unwrap()[3] & 0x0F, NOTIMP as u8);
    }

    #[test]
    fn an_address_goes_to_another_name_only_after_the_pool_went_round_and_it_went_stale() {
        let mut dns = FakeDns::new();
        let t0 = Instant::now();
        let first = dns.address_at("first.example", t0).unwrap();
        for i in 0..LAST - FIRST {
            assert_ne!(dns.address_at(&format!("n{i}.example"), t0), Some(first));
        }
        assert_eq!(
            dns.address_at("late.example", t0 + REUSE_AFTER / 2),
            None,
            "the whole pool is still fresh"
        );
        assert_eq!(dns.name_of(first), Some("first.example"));

        let later = t0 + REUSE_AFTER;
        assert_eq!(dns.address_at("late.example", later), Some(first));
        assert_eq!(dns.name_of(first), Some("late.example"));
        assert_ne!(
            dns.address_at("first.example", later),
            Some(first),
            "the old name is resolved afresh"
        );
    }

    #[test]
    fn asking_again_keeps_an_address_and_the_stale_ones_go_first() {
        let mut dns = FakeDns::new();
        let t0 = Instant::now();
        let kept = dns.address_at("kept.example", t0).unwrap();
        for i in 0..LAST - FIRST {
            dns.address_at(&format!("n{i}.example"), t0);
        }
        let later = t0 + REUSE_AFTER;
        assert_eq!(
            dns.address_at("kept.example", later - Duration::from_secs(1)),
            Some(kept)
        );
        let fresh = dns.address_at("new.example", later);
        assert!(
            fresh.is_some(),
            "a name asked for again does not block the pool"
        );
        assert_ne!(fresh, Some(kept));
        assert_eq!(dns.name_of(kept), Some("kept.example"));
    }

    #[test]
    fn a_name_that_cannot_get_an_address_yet_gets_servfail() {
        let mut dns = FakeDns::new();
        let t0 = Instant::now();
        for i in 0..=LAST - FIRST {
            dns.address_at(&format!("n{i}.example"), t0);
        }
        let r = dns
            .answer_at(&query(5, "one-more.example", TYPE_A), t0)
            .unwrap();
        assert_eq!(r[3] & 0x0F, SERVFAIL as u8);
        assert_eq!(r[6..8], [0, 0], "no address");
    }

    #[test]
    fn the_pool_is_198_18_slash_15_and_the_resolver_is_in_it() {
        assert!(FakeDns::is_fake(Ipv4Addr::new(198, 18, 0, 5)));
        assert!(FakeDns::is_fake(Ipv4Addr::new(198, 19, 255, 255)));
        assert!(FakeDns::is_fake(RESOLVER));
        assert!(!FakeDns::is_fake(Ipv4Addr::new(198, 20, 0, 1)));
        assert!(!FakeDns::is_fake(Ipv4Addr::new(10, 0, 0, 1)));
    }
}

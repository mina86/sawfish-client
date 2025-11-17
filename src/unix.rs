// sawfish-client -- client library to communicate with Sawfish window manager
// © 2025 by Michał Nazarewicz <mina86@mina86.com>

use std::borrow::Cow;
use std::ffi::{OsString, c_ulong};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use crate::{ConnError, EvalError, EvalResponse};

/// A Unix-socket-based connection to the Sawfish server.
pub struct Client(std::os::unix::net::UnixStream);

impl Client {
    /// Opens connection to Sawfish through a Unix socket.
    ///
    /// The path of Unix socket is `/tmp/.sawfish-{logname}/{display}` where
    /// `{logname}` is value of `LOGNAME` environment variable and `{display}`
    /// is a canonical display name.
    pub fn open(display: &str) -> Result<Self, ConnError> {
        let username =
            std::env::var_os("LOGNAME").ok_or(ConnError::NoLogname)?;
        let path = [
            "/tmp/.sawfish-".as_bytes(),
            username.as_encoded_bytes(),
            "/".as_bytes(),
            canonical_display(display).as_bytes(),
        ]
        .concat();
        // SAFETY: Concatenating Strings and OsStrings produces valid OsStrings.
        let path = unsafe { OsString::from_encoded_bytes_unchecked(path) };
        let path = std::path::PathBuf::from(path);
        UnixStream::connect(&path)
            .map(Self)
            .map_err(|err| ConnError::Io(path, err))
    }

    /// Sends form to the server for evaluation and waits for response if
    /// requested.
    pub fn eval(
        &mut self,
        form: &[u8],
        is_async: bool,
    ) -> Result<crate::EvalResponse, EvalError> {
        self.send_request(form, is_async)?;
        if is_async { Ok(Ok(Vec::new())) } else { self.read_response() }
    }

    /// Sends request to the server.
    ///
    /// If `is_async` is `false`, the caller is responsible for calling
    /// [`Self::read_response`].  Otherwise, the requests and responses will get
    /// out of sync.
    fn send_request(
        &mut self,
        form: &[u8],
        is_async: bool,
    ) -> Result<(), EvalError> {
        let req_type = is_async as u8;
        let req_len = c_ulong::try_from(form.len()).unwrap();
        let mut buf = [0u8; 9];
        buf[0] = req_type;
        buf[1..].copy_from_slice(&req_len.to_ne_bytes());
        self.0.write_all(&buf)?;
        self.0.write_all(form)?;
        Ok(())
    }

    /// Reads response from the server.
    fn read_response(&mut self) -> Result<EvalResponse, EvalError> {
        let mut buf = [0u8; core::mem::size_of::<c_ulong>()];
        self.0.read_exact(&mut buf)?;
        let res_len = c_ulong::from_ne_bytes(buf);
        if res_len == 0 {
            return Err(EvalError::NoResponse.into());
        }
        let data_len = usize::try_from(res_len - 1)
            .map_err(|_| EvalError::ResponseTooLarge(res_len - 1))?;

        let mut state = 0u8;
        self.0.read_exact(core::slice::from_mut(&mut state))?;

        let mut response = vec![0u8; data_len];
        self.0.read_exact(&mut response)?;
        Ok(if state == 1 { Ok(response) } else { Err(response) })
    }
}


/// System's canonical hostname.
static SYSTEM_NAME: std::sync::LazyLock<Option<String>> =
    std::sync::LazyLock::new(get_system_name);

/// Returns canonical system name, i.e. a fully-qualified hostname of the host.
fn get_system_name() -> Option<String> {
    if cfg!(test) {
        Some("host.local".into())
    } else {
        let host = dns_lookup::get_hostname().ok()?;
        if !host.contains('.') &&
            let Some(host) = canonical_host_impl(&host)
        {
            return Some(host);
        }
        Some(host)
    }
}

/// Returns the canonical, fully-qualified, lowercase version of the hostname.
fn canonical_host(host: &str) -> String {
    canonical_host_impl(host).as_deref().unwrap_or(host).to_lowercase()
}

fn canonical_host_impl(host: &str) -> Option<String> {
    if cfg!(test) {
        Some(if host == "nofq" {
            host.into()
        } else if host.contains('.') {
            host.to_lowercase()
        } else {
            host.to_lowercase() + ".local"
        })
    } else {
        let hints = dns_lookup::AddrInfoHints {
            flags: libc::AI_CANONNAME,
            address: 0,
            socktype: 0,
            protocol: 0,
        };
        let iter = dns_lookup::getaddrinfo(Some(host), None, Some(hints));
        if let Ok(iter) = iter {
            for info in iter {
                if let Some(name) = info.ok().and_then(|info| info.canonname) &&
                    name.contains('.')
                {
                    return Some(name);
                }
            }
        }
        None
    }
}

/// Returns the canonical display string (e.g. `":0"` → `"example.com:0.0"`).
fn canonical_display(mut name: &str) -> String {
    if name.starts_with("unix:") {
        name = &name[4..];
    }
    let (host, rest) = name.split_once(':').unwrap_or((name, "0"));
    let host = if host.is_empty() {
        SYSTEM_NAME.as_deref().map(Cow::Borrowed)
    } else {
        Some(Cow::Owned(canonical_host(host)))
    };
    let host = host.as_deref().unwrap_or("");
    let (display, screen) = rest.split_once('.').unwrap_or((rest, "0"));
    format!("{host}:{display}.{screen}")
}

#[test]
fn test_canonical_dispaly() {
    for (display, canonical) in [
        ("", "host.local:0.0"),
        (":0", "host.local:0.0"),
        (":0.1", "host.local:0.1"),
        ("host:0", "host.local:0.0"),
        ("host.example.com:0", "host.example.com:0.0"),
        ("nofq:0", "nofq:0.0"),
        ("bogus", "bogus.local:0.0"),
    ] {
        assert_eq!(canonical, canonical_display(display), "{display}");
    }
}

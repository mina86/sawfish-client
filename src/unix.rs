// sawfish-client -- client library to communicate with Sawfish window manager
// © 2025 by Michał Nazarewicz <mina86@mina86.com>

use std::borrow::Cow;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

#[cfg(feature = "async")]
use futures_util::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{ConnError, EvalError, EvalResponse};

/// A Unix-socket-based connection to the Sawfish server.
pub struct Client(std::os::unix::net::UnixStream);

/// Returns path to the Unix socket Sawfish server is listening on.
///
/// The path of Unix socket is `/tmp/.sawfish-{logname}/{display}` where
/// `{logname}` is value of `LOGNAME` environment variable and `{display}`
/// is a canonical display name.
pub fn server_path(display: &str) -> Result<std::path::PathBuf, ConnError> {
    let username = std::env::var_os("LOGNAME").ok_or(ConnError::NoLogname)?;
    let path = [
        "/tmp/.sawfish-".as_bytes(),
        username.as_encoded_bytes(),
        "/".as_bytes(),
        canonical_display(display).as_bytes(),
    ]
    .concat();
    // SAFETY: Concatenating Strings and OsStrings produces valid OsStrings.
    let path = unsafe { OsString::from_encoded_bytes_unchecked(path) };
    Ok(std::path::PathBuf::from(path))
}

impl Client {
    /// Opens connection to Sawfish through a Unix socket at given location.
    pub fn open(display: &str) -> Result<Self, ConnError> {
        let path = server_path(display)?;
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
    ) -> Result<EvalResponse, EvalError> {
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
        let req_len = u64::try_from(form.len()).unwrap();
        let mut buf = [0u8; 9];
        buf[0] = req_type;
        buf[1..].copy_from_slice(&req_len.to_ne_bytes());
        self.0.write_all(&buf)?;
        self.0.write_all(form)?;
        Ok(())
    }

    /// Reads response from the server.
    fn read_response(&mut self) -> Result<EvalResponse, EvalError> {
        let mut buf = [0u8; 8];
        self.0.read_exact(&mut buf)?;
        let res_len = u64::from_ne_bytes(buf);
        if res_len == 0 {
            return Err(EvalError::NoResponse);
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


/// A Unix-socket-based connection to the Sawfish server using async I/O.
#[cfg(feature = "async")]
pub struct AsyncClient<S>(pub S);

#[cfg(feature = "async")]
impl<S: AsyncRead + AsyncWrite + Unpin> AsyncClient<S> {
    /// Sends form to the server for evaluation and waits for response if
    /// requested.
    pub async fn eval(
        &mut self,
        form: &[u8],
        is_async: bool,
    ) -> Result<crate::EvalResponse, EvalError> {
        self.send_request(form, is_async).await?;
        if is_async { Ok(Ok(Vec::new())) } else { self.read_response().await }
    }

    /// Sends request to the server.
    ///
    /// If `is_async` is `false`, the caller is responsible for calling
    /// [`Self::read_response`].  Otherwise, the requests and responses will get
    /// out of sync.
    async fn send_request(
        &mut self,
        form: &[u8],
        is_async: bool,
    ) -> Result<(), EvalError> {
        let req_type = is_async as u8;
        let req_len = u64::try_from(form.len()).unwrap();
        let mut buf = [0u8; 9];
        buf[0] = req_type;
        buf[1..].copy_from_slice(&req_len.to_ne_bytes());
        let mut bufs =
            [std::io::IoSlice::new(&buf), std::io::IoSlice::new(form)];
        self.0.write_all_vectored(&mut bufs).await.map_err(EvalError::from)
    }

    /// Reads response from the server.
    async fn read_response(&mut self) -> Result<EvalResponse, EvalError> {
        let mut buf = [0u8; 8];
        self.0.read_exact(&mut buf).await?;
        let res_len = u64::from_ne_bytes(buf);
        if res_len == 0 {
            return Err(EvalError::NoResponse);
        }
        let data_len = usize::try_from(res_len - 1)
            .map_err(|_| EvalError::ResponseTooLarge(res_len - 1))?;

        let mut state = 0u8;
        self.0.read_exact(core::slice::from_mut(&mut state)).await?;

        let mut response = vec![0u8; data_len];
        self.0.read_exact(&mut response).await?;
        Ok(if state == 1 { Ok(response) } else { Err(response) })
    }
}


#[cfg(test)]
mod test_eval {
    use std::os::unix::net::UnixStream;

    use super::*;

    fn server_thread(mut server: UnixStream) -> () {
        let mut buf = [0; 32];
        let mut pos = 0;
        loop {
            match server.read(&mut buf[pos..]) {
                Ok(0) => break,
                Ok(n) => pos += n,
                Err(err) => {
                    if err.kind() != std::io::ErrorKind::WouldBlock &&
                        err.kind() != std::io::ErrorKind::TimedOut
                    {
                        panic!("{err}");
                    }
                    assert_eq!(
                        0,
                        pos,
                        "Server timed out with data left: {:?}",
                        &buf[..pos]
                    );
                    break;
                }
            }
            if pos < 9 {
                continue;
            }

            let len = u64::from_ne_bytes(buf[1..9].try_into().unwrap());
            let len = usize::try_from(len).unwrap();
            let response = match (buf[0], buf[9..].get(..len)) {
                (_, None) => continue,
                (0, Some(b"ok")) => Some(Ok(())),
                (0, Some(b"err")) => Some(Err(())),
                (1, Some(b"async")) => None,
                (is_async, Some(form)) => panic!(
                    "Invalid requset: is_async: {is_async}; form: {form:?}"
                ),
            };

            if let Some(response) = response {
                let mut buf = *b"\x09\0\0\0\0\0\0\0\xffresponse";
                buf[8] = response.is_ok() as u8;
                server.write_all(&buf).unwrap();
            }

            buf.copy_within(len + 9.., 0);
            pos -= len + 9;
        }
    }

    fn start_test(name: &str) -> (UnixStream, std::thread::JoinHandle<()>) {
        const SECOND: std::time::Duration = std::time::Duration::new(1, 0);

        let (client, server) = UnixStream::pair().unwrap();
        client.set_read_timeout(Some(SECOND)).unwrap();
        client.set_write_timeout(Some(SECOND)).unwrap();
        server.set_read_timeout(Some(SECOND)).unwrap();
        server.set_write_timeout(Some(SECOND)).unwrap();

        let server = std::thread::Builder::new()
            .name(format!("test-{name}-server"))
            .spawn(move || server_thread(server))
            .unwrap();

        (client, server)
    }

    #[track_caller]
    fn do_test(want: Result<&str, &str>, form: &str, is_async: bool) {
        let (client, server) = start_test(form);
        let mut client = Client(client);
        let got = client.eval(form.as_bytes(), is_async);
        client.0.shutdown(std::net::Shutdown::Both).unwrap();
        core::mem::drop(client);
        server.join().unwrap();

        let got = got
            .unwrap()
            .map(|bytes| String::from_utf8(bytes).unwrap())
            .map_err(|bytes| String::from_utf8(bytes).unwrap());
        assert_eq!(want, got.as_deref().map_err(String::as_str));
    }

    #[test]
    fn test_eval_ok() { do_test(Ok("response"), "ok", false); }

    #[test]
    fn test_eval_err() { do_test(Err("response"), "err", false); }

    #[test]
    fn test_eval_async() { do_test(Ok(""), "async", true); }

    #[cfg(feature = "async")]
    #[track_caller]
    fn do_async_test(want: Result<&str, &str>, form: &str, is_async: bool) {
        use tokio_util::compat::TokioAsyncReadCompatExt;

        let (client, server) = start_test(form);
        client.set_nonblocking(true).unwrap();

        let got = {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()
                .unwrap();
            let _guerd = rt.enter();

            let client = tokio::net::UnixStream::from_std(client).unwrap();
            let mut client = AsyncClient(client.compat());
            rt.block_on(async {
                let got = client.eval(form.as_bytes(), is_async).await;
                client
                    .0
                    .into_inner()
                    .into_std()
                    .unwrap()
                    .shutdown(std::net::Shutdown::Both)
                    .unwrap();
                got
            })
        };
        server.join().unwrap();

        let got = got
            .unwrap()
            .map(|bytes| String::from_utf8(bytes).unwrap())
            .map_err(|bytes| String::from_utf8(bytes).unwrap());
        assert_eq!(want, got.as_deref().map_err(String::as_str));
    }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_eval_ok() { do_async_test(Ok("response"), "ok", false); }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_eval_err() { do_async_test(Err("response"), "err", false); }

    #[cfg(feature = "async")]
    #[test]
    fn test_async_eval_async() { do_async_test(Ok(""), "async", true); }
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

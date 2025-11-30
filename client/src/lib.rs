// sawfish-client -- client library to communicate with Sawfish window manager
// © 2025 by Michał Nazarewicz <mina86@mina86.com>
//
// sawfish-client is free software: you can redistribute it and/or modify it
// under the terms of the GNU Lesser General Public License as published by the
// Free Software Foundation; either version 3 of the License, or (at your
// option) any later version.
//
// sawfish-client is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along with
// sawfish-client.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

use std::borrow::Cow;

#[cfg(feature = "async")]
use futures_util::io::{AsyncRead, AsyncWrite};

mod error;
mod unix;
#[cfg(feature = "experimental-xcb")]
mod x11;

pub use error::{ConnError, EvalError};

/// A connection to the Sawfish window manager.
pub struct Client(Inner);

/// Result of a form evaluation.
///
/// If the form was successfully evaluated, the response from the server (with
/// the value the form evaluated to) is represented by the `Ok` variant.  If the
/// form failed to evaluated (most likely due to syntax error), the error
/// message is represented by the `Err` variant.
pub type EvalResponse = Result<Vec<u8>, Vec<u8>>;

enum Inner {
    Unix(unix::Client),
    X11(x11::Client),
}

impl Client {
    /// Opens a connection to the Sawfish server.
    ///
    /// The `display` argument specifies an optional display string, (such as
    /// `":0"`).  If not provided, the `DISPLAY` environment variable is used.
    ///
    /// Tries to connect to the Unix socket of the Sawfish server.  If that
    /// fails and the `experimental-xcb` Cargo feature is enabled, tries using
    /// X11 protocol to communicate with Sawfish.
    pub fn open(display: Option<&str>) -> Result<Self, ConnError> {
        let display = get_display(display)?;
        match unix::Client::open(&display) {
            Ok(client) => Ok(Self(Inner::Unix(client))),
            Err(err) => x11::Client::fallback(&display, err)
                .map(|client| Self(Inner::X11(client))),
        }
    }

    /// Sends a Lisp `form` to the Sawfish server for evaluation and waits for
    /// a reply.
    ///
    /// * If there’s an error sending the `form` to the server (e.g. an I/O
    ///   error), returns an `Err(error)` value.
    /// * Otherwise, if the `form` has been successfully sent to the server but
    ///   evaluation failed, returns `Ok(Err(data))` value.
    /// * Otherwise, if the `form` has been successfully executed by the server,
    ///   returns `Ok(Ok(data))` value.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let mut client = sawfish_client::Client::open(None).unwrap();
    /// match client.eval("(system-name)") {
    ///     Ok(Ok(data)) => {
    ///         println!("Form evaluated to: {}",
    ///                  String::from_utf8_lossy(&data))
    ///     }
    ///     Ok(Err(data)) => {
    ///         println!("Error evaluating form: {}",
    ///                  String::from_utf8_lossy(&data))
    ///     }
    ///     Err(err) => println!("Communication error: {err}")
    /// }
    /// ```
    pub fn eval(
        &mut self,
        form: impl AsRef<[u8]>,
    ) -> Result<EvalResponse, EvalError> {
        match &mut self.0 {
            Inner::Unix(client) => client.eval(form.as_ref(), false),
            Inner::X11(client) => client.eval(form.as_ref(), false),
        }
    }

    /// Sends a Lisp `form` to the Sawfish server for evaluation but does not
    /// wait for a reply.
    ///
    /// If there’s an error sending the `form` to the server (e.g. an I/O
    /// error), returns an `Err(error)` value.  Otherwise, so long as the `form`
    /// was successfully sent, returns `Ok(())` even if evaluation on the server
    /// side has changed (e.g. due to syntax error).  Use [`Self::eval`] instead
    /// to check whether evaluation succeeded.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let mut client = sawfish_client::Client::open(None).unwrap();
    /// match client.send("(set-screen-viewport 0 0)") {
    ///     Ok(()) => println!("Form successfully sent"),
    ///     Err(err) => println!("Communication error: {err}")
    /// }
    /// ```
    pub fn send(&mut self, form: impl AsRef<[u8]>) -> Result<(), EvalError> {
        match &mut self.0 {
            Inner::Unix(client) => client.eval(form.as_ref(), true).map(|_| ()),
            Inner::X11(client) => client.eval(form.as_ref(), true).map(|_| ()),
        }
    }
}

/// Opens a connection to the Sawfish server.
///
/// This is a convenience alias for [`Client::open`].
#[inline]
pub fn open(display: Option<&str>) -> Result<Client, ConnError> {
    Client::open(display)
}


/// A connection to the Sawfish window manager using asynchronous I/O.
#[cfg(feature = "async")]
pub struct AsyncClient<S>(unix::AsyncClient<S>);

/// An alias for the [`AsyncClient`] which uses Tokio runtime Unix stream.
///
/// # Example
///
/// ```no_run
/// use tokio_util::compat::TokioAsyncReadCompatExt;
///
/// async fn print_system_name() {
///     let mut client = sawfish_client::open_tokio(None).await.unwrap();
///     let sysname = client.eval("(system-name)").await.unwrap().unwrap();
///     println!("{}", String::from_utf8_lossy(&sysname));
/// }
/// ```
#[cfg(feature = "tokio")]
pub type TokioClient =
    AsyncClient<tokio_util::compat::Compat<tokio::net::UnixStream>>;

#[cfg(feature = "tokio")]
impl AsyncClient<tokio_util::compat::Compat<tokio::net::UnixStream>> {
    /// Opens a connection to the Sawfish server using the Tokio runtime.
    ///
    /// The `display` argument specifies an optional display string, (such as
    /// `":0"`).  If not provided, the `DISPLAY` environment variable is used.
    pub async fn open(display: Option<&str>) -> Result<Self, ConnError> {
        let display = get_display(display)?;
        unix::AsyncClient::open(&display).await.map(Self)
    }
}

/// Opens a connection to the Sawfish server using the Tokio runtime.
///
/// This is a convenience alias for [`AsyncClient::open`] with the generic
/// argument `S` set to Tokio Unix stream type.
#[cfg(feature = "tokio")]
#[inline]
pub async fn open_tokio(
    display: Option<&str>,
) -> Result<TokioClient, ConnError> {
    TokioClient::open(display).await
}

#[cfg(feature = "async")]
impl<S: AsyncRead + AsyncWrite + Unpin> AsyncClient<S> {
    /// Constructs a connection to the Sawfish server over an asynchronous Unix
    /// socket.
    ///
    /// Because the creation of an asynchronous Unix socket depends on the async
    /// runtime, responsibility to open the connection falls on the caller.  Use
    /// [`server_path`] to determine path to the Unix Socket the Sawfish server
    /// is (supposed to be) listening on.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tokio_util::compat::TokioAsyncReadCompatExt;
    ///
    /// type TokioClient = sawfish_client::AsyncClient<
    ///     tokio_util::compat::Compat<tokio::net::UnixStream>>;
    ///
    /// async fn open() -> TokioClient {
    ///     let path = sawfish_client::server_path(None).unwrap();
    ///     let sock = tokio::net::UnixStream::connect(path).await.unwrap();
    ///     sawfish_client::AsyncClient::new(sock.compat())
    /// }
    /// ```
    pub fn new(socket: S) -> Self { Self(unix::AsyncClient(socket)) }

    /// Sends a Lisp `form` to the Sawfish server for evaluation and waits for
    /// a reply.
    ///
    /// * If there’s an error sending the `form` to the server (e.g. an I/O
    ///   error), returns an `Err(error)` value.
    /// * Otherwise, if the `form` has been successfully sent to the server but
    ///   evaluation failed (e.g. due to syntax error), returns `Ok(Err(data))`
    ///   value.
    /// * Otherwise, if the `form` has been successfully executed by the server,
    ///   returns `Ok(Ok(data))` value.
    ///
    /// # Example
    ///
    /// ```
    /// use futures_util::{AsyncRead, AsyncWrite};
    ///
    /// async fn system_name<S: AsyncRead + AsyncWrite + Unpin>(
    ///     client: &mut sawfish_client::AsyncClient<S>,
    /// ) -> Option<String> {
    ///     match client.eval("(system-name)").await {
    ///         Ok(Ok(data)) => {
    ///             Some(String::from_utf8_lossy(&data).into_owned())
    ///         }
    ///         Ok(Err(data)) => {
    ///             println!("Error evaluating form: {}",
    ///                      String::from_utf8_lossy(&data));
    ///             None
    ///         }
    ///         Err(err) => {
    ///             println!("Communication error: {err}");
    ///             None
    ///         }
    ///     }
    /// }
    /// ```
    pub async fn eval(
        &mut self,
        form: impl AsRef<[u8]>,
    ) -> Result<EvalResponse, EvalError> {
        self.0.eval(form.as_ref(), false).await
    }

    /// Sends a Lisp `form` to the Sawfish server for evaluation but does not
    /// wait for a reply.
    ///
    /// If there’s an error sending the `form` to the server (e.g. an I/O
    /// error), returns an `Err(error)` value.  Otherwise, so long as the `form`
    /// was successfully sent, returns `Ok(())` even if evaluation on the server
    /// side has changed (e.g. due to syntax error).  Use [`Self::eval`] instead
    /// to check whether evaluation succeeded.
    ///
    /// # Example
    ///
    /// ```
    /// use futures_util::{AsyncRead, AsyncWrite};
    ///
    /// async fn set_screen_viewport<S: AsyncRead + AsyncWrite + Unpin>(
    ///     client: &mut sawfish_client::AsyncClient<S>,
    ///     x: u32,
    ///     y: u32,
    /// ) {
    ///     let form = format!("(set-screen-viewport {x} {y})");
    ///     if let Err(err) = client.send(&form).await {
    ///         println!("Communication error: {err}");
    ///     }
    /// }
    /// ```
    pub async fn send(
        &mut self,
        form: impl AsRef<[u8]>,
    ) -> Result<(), EvalError> {
        self.0.eval(form.as_ref(), true).await.map(|_| ())
    }
}


/// Returns path of the Unix socket the Sawfish server is (or should be)
/// listening on.
///
/// Does not verify that the socket exists or the Sawfish server is listening on
/// it.  This is used for opening connections with [`AsyncClient::new`].
///
/// The Unix socket is located in `/tmp/.sawfish-$LOGNAME` directory.
#[cfg(feature = "async")]
pub fn server_path(
    display: Option<&str>,
) -> Result<std::path::PathBuf, ConnError> {
    get_display(display).and_then(|display| unix::server_path(&display))
}


/// Unwraps the option or returns value of $DISPLAY environment variable.
fn get_display(
    display: Option<&str>,
) -> Result<std::borrow::Cow<'_, str>, ConnError> {
    display
        .map(Cow::Borrowed)
        .or_else(|| std::env::var("DISPLAY").map(Cow::Owned).ok())
        .filter(|display| !display.is_empty())
        .ok_or(ConnError::NoDisplay)
}


#[cfg(not(feature = "experimental-xcb"))]
mod x11 {
    use super::*;

    pub enum Client {}

    impl Client {
        pub fn fallback(
            _display: &str,
            err: ConnError,
        ) -> Result<Self, ConnError> {
            Err(err)
        }

        pub fn eval(
            &mut self,
            _form: &[u8],
            _is_async: bool,
        ) -> Result<EvalResponse, EvalError> {
            match *self {}
        }
    }
}

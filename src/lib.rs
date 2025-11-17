// sawfish-client -- client library to communicate with Sawfish window manager
// © 2025 by Michał Nazarewicz <mina86@mina86.com>
//
// setroot is free software: you can redistribute it and/or modify it under the
// terms of the GNU Lesser General Public License as published by the Free
// Software Foundation; either version 3 of the License, or (at your option) any
// later version.
//
// setroot is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR
// A PARTICULAR PURPOSE.  See the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with
// sawfish-client.  If not, see <http://www.gnu.org/licenses/>.

#![doc = include_str!("../README.md")]

use std::borrow::Cow;

mod error;
mod unix;
#[cfg(feature = "experimental-xcb")]
mod x11;

pub use error::{ConnError, EvalError};

/// A connection to the the Sawfish window manager.
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
        let display = display
            .map(Cow::Borrowed)
            .or_else(|| std::env::var("DISPLAY").map(Cow::Owned).ok())
            .filter(|display| !display.is_empty())
            .ok_or(ConnError::NoDisplay)?;
        match unix::Client::open(&display) {
            Ok(conn) => Ok(Self(Inner::Unix(conn))),
            Err(err) => x11::Client::fallback(&display, err)
                .map(|conn| Self(Inner::X11(conn))),
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
    /// let mut conn = sawfish_client::Client::open(None).unwrap();
    /// match conn.eval("(system-name)") {
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
            Inner::Unix(conn) => conn.eval(form.as_ref(), false),
            Inner::X11(conn) => conn.eval(form.as_ref(), false),
        }
    }

    /// Sends a Lisp `form` to the Sawfish server for evaluation but does not
    /// wait for a reply.
    ///
    /// Note that ‘async’ nomenclature comes from Sawfish and is not related to
    /// Rust’s concept of `async` functions.  The form is sent to Sawfish using
    /// blocking I/O with the difference from [`Self::eval`] being that no
    /// response from Sawfish is read.
    pub fn eval_async(
        &mut self,
        form: impl AsRef<[u8]>,
    ) -> Result<(), EvalError> {
        match &mut self.0 {
            Inner::Unix(conn) => conn.eval(form.as_ref(), true).map(|_| ()),
            Inner::X11(conn) => conn.eval(form.as_ref(), true).map(|_| ()),
        }
    }
}

/// Opens a connection to the Sawfish server.
///
/// This is a convenience alias for [`Client::open`].
#[inline(always)]
pub fn open(display: Option<&str>) -> Result<Client, ConnError> {
    Client::open(display)
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

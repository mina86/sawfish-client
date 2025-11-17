// sawfish-client -- client library to communicate with Sawfish window manager
// © 2025 by Michał Nazarewicz <mina86@mina86.com>

#[cfg(feature = "experimental-xcb")]
use xcb::x;

/// Error during establishing connection to the Sawfish server.
#[derive(Debug, derive_more::From)]
#[non_exhaustive]
pub enum ConnError {
    /// No display specified and DISPLAY environment variable not set.
    NoDisplay,
    /// LOGNAME environment variable not set.
    ///
    /// This is relevant when connecting to Unix socket since without the login
    /// name socket name cannot be determined.
    NoLogname,
    /// An I/O error during establishing of the connection (e.g. Unix socket
    /// does not exist or user lacks permissions to access it).
    Io(std::path::PathBuf, std::io::Error),
    /// Invalid X11 display screen number.
    #[cfg(feature = "experimental-xcb")]
    BadScreen(i32),
    /// No Sawfish server found on display.
    #[cfg(feature = "experimental-xcb")]
    ServerNotFound,
    /// An X11 error during establishing of the connection.
    #[cfg(feature = "experimental-xcb")]
    #[from(xcb::Error, xcb::ConnError, xcb::ProtocolError)]
    X11(xcb::Error),
}

impl core::fmt::Display for ConnError {
    fn fmt(&self, fmtr: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NoDisplay => {
                "No display specified and DISPLAY variable not set".fmt(fmtr)
            }
            Self::NoLogname => "LOGNAME environment variable not set".fmt(fmtr),
            #[cfg(feature = "experimental-xcb")]
            Self::BadScreen(screen) => {
                write!(fmtr, "Invalid screen number {screen}")
            }
            #[cfg(feature = "experimental-xcb")]
            Self::ServerNotFound => {
                "No Sawfish server found on X11 screen".fmt(fmtr)
            }
            #[cfg(feature = "experimental-xcb")]
            Self::X11(err) => err.fmt(fmtr),
            Self::Io(path, err) => write!(fmtr, "{}: {}", path.display(), err),
        }
    }
}


/// Error during sending form for evaluation.
#[derive(Debug, derive_more::From)]
#[non_exhaustive]
pub enum EvalError {
    /// Got empty response to non-async request.
    ///
    /// Note that this is different than the response data being empty.  The
    /// data, what [`crate::Client::eval`] returns in `Ok` variant, may be empty
    /// and that’s not considered an error.
    NoResponse,
    /// Response too large to handle.  This can only happen on systems where
    /// `usize` is smaller than 64-bit.
    ResponseTooLarge(std::ffi::c_ulong),
    /// An I/O error during communication with the Sawfish server.
    #[from(std::io::Error)]
    Io(std::io::Error),
    /// Invalid format of the window’s response property.
    #[cfg(feature = "experimental-xcb")]
    BadResponse {
        /// The portal window where the response was read from.
        window: x::Window,
        /// The atom identifier of the property with the response.
        atom: x::Atom,
        /// The actual type of the response property (an atom), see
        /// [`x::GetPropertyReply::type`].
        typ: x::Atom,
        /// The actual format of the response property, see
        /// [`x::GetPropertyReply::format`].
        format: u8,
    },
    /// X11 error during communication with Sawfish server.
    #[cfg(feature = "experimental-xcb")]
    #[from(xcb::Error, xcb::ConnError, xcb::ProtocolError)]
    X11(xcb::Error),
}

impl core::fmt::Display for EvalError {
    fn fmt(&self, fmtr: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NoResponse => "No response to non-async request".fmt(fmtr),
            Self::ResponseTooLarge(len) => {
                write!(fmtr, "Response of {len} bytes too large")
            }
            Self::Io(err) => err.fmt(fmtr),
            #[cfg(feature = "experimental-xcb")]
            Self::BadResponse { window, atom, typ, format } => {
                use xcb::Xid;
                write!(
                    fmtr,
                    "Invalid format of response property (window:{}, atom:{}, \
                     typ:{}, format:{})",
                    window.resource_id(),
                    atom.resource_id(),
                    typ.resource_id(),
                    format
                )
            }
            #[cfg(feature = "experimental-xcb")]
            Self::X11(err) => err.fmt(fmtr),
        }
    }
}


impl std::error::Error for ConnError {}
impl std::error::Error for EvalError {}

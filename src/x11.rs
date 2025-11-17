// sawfish-client -- client library to communicate with Sawfish window manager
// © 2025 by Michał Nazarewicz <mina86@mina86.com>

use xcb::x::PropEl;
use xcb::{Xid, x};

use crate::{ConnError, EvalError, EvalResponse};

const PROTOCOL_X11_VERSION: u32 = 1;

pub struct Client {
    conn: xcb::Connection,
    req_win: x::Window,
    portal: x::Window,
    property: x::Atom,
}

impl Client {
    /// Opens connection to Sawfish through X11 property protocol.
    ///
    /// The purpose of the method is to simplify conditional compilation.  When
    /// the crate is built without XCB support, a fallback implementation of
    /// this function returns the error.  This eliminates conditional
    /// compilation from the caller.
    pub fn fallback(display: &str, _err: ConnError) -> Result<Self, ConnError> {
        Self::open(display)
    }

    /// Opens connection to Sawfish through X11 property protocol.
    pub fn open(display: &str) -> Result<Self, ConnError> {
        let (conn, screen) = xcb::Connection::connect(Some(display))?;
        let setup = conn.get_setup();
        let screen = usize::try_from(screen)
            .ok()
            .and_then(|idx| setup.roots().nth(idx))
            .ok_or(ConnError::BadScreen(screen))?;
        let root = screen.root();

        // Intern needed atoms.
        let cookie = conn.send_request(&x::InternAtom {
            only_if_exists: true,
            name: "_SAWFISH_REQUEST_WIN".as_bytes(),
        });
        let req_win_atom = conn.wait_for_reply(cookie)?.atom();
        if req_win_atom.is_none() {
            return Err(ConnError::ServerNotFound);
        }

        let cookie = conn.send_request(&x::InternAtom {
            only_if_exists: false,
            name: "_SAWFISH_REQUEST".as_bytes(),
        });
        let property = conn.wait_for_reply(cookie)?.atom();

        // Get the server's request window ID from the root window property
        let reply =
            conn.wait_for_reply(conn.send_request(&x::GetProperty {
                delete: false,
                window: root,
                property: req_win_atom,
                r#type: x::ATOM_CARDINAL,
                long_offset: 0,
                long_length: 1,
            }))?;

        // Validate property type and format
        if reply.r#type() != x::ATOM_CARDINAL ||
            reply.format() != x::Window::FORMAT ||
            reply.length() != 1
        {
            return Err(ConnError::ServerNotFound);
        }
        let req_win = reply.value::<x::Window>()[0];

        // Create the portal window (private communication window)
        let portal = conn.generate_id();
        conn.send_and_check_request(&x::CreateWindow {
            depth: x::COPY_FROM_PARENT as u8,
            wid: portal,
            parent: root,
            x: -100,
            y: -100,
            width: 10,
            height: 10,
            border_width: 0,
            class: x::WindowClass::InputOutput,
            visual: x::COPY_FROM_PARENT,
            value_list: &[x::Cw::EventMask(x::EventMask::PROPERTY_CHANGE)],
        })?;

        Ok(Self { conn, req_win, portal, property })
    }

    /// Sends form to the server for evaluation and waits for response if
    /// requested.
    pub fn eval(
        &mut self,
        form: &[u8],
        is_async: bool,
    ) -> Result<EvalResponse, EvalError> {
        self.send_request(form, is_async).map_err(std::io::Error::other)?;
        if is_async {
            self.conn.flush().map_err(std::io::Error::other)?;
            Ok(Ok(Vec::new()))
        } else {
            self.wait_for_property_notify().map_err(std::io::Error::other)?;
            self.read_response()
        }
    }

    /// Sends request to the server.
    fn send_request(
        &mut self,
        form: &[u8],
        is_async: bool,
    ) -> Result<(), xcb::Error> {
        // Set the property on the portal window to the form.
        self.conn.send_and_check_request(&x::ChangeProperty {
            mode: x::PropMode::Replace,
            window: self.portal,
            property: self.property,
            r#type: x::ATOM_STRING,
            data: form,
        })?;
        // Swallow the PropertyNotify event resulting from us changing the
        // property..
        self.wait_for_property_notify()?;

        // Send request to Sawfish server.
        let event = x::ClientMessageEvent::new(
            self.req_win,
            self.property,
            x::ClientMessageData::Data32([
                PROTOCOL_X11_VERSION,
                self.portal.resource_id(),
                self.property.resource_id(),
                if is_async { 0 } else { 1 },
                0,
            ]),
        );
        self.conn.send_and_check_request(&x::SendEvent {
            propagate: false,
            destination: x::SendEventDest::Window(self.req_win),
            event_mask: x::EventMask::NO_EVENT,
            event: &event,
        })?;
        Ok(())
    }

    /// Reads response from the server.
    fn read_response(&mut self) -> Result<EvalResponse, EvalError> {
        let mut long_length = 16u32;
        let (success, data) = loop {
            let cookie = self.conn.send_request(&x::GetProperty {
                delete: false,
                window: self.portal,
                property: self.property,
                r#type: x::ATOM_STRING,
                long_offset: 0,
                long_length,
            });
            let reply = self
                .conn
                .wait_for_reply(cookie)
                .map_err(std::io::Error::other)?;
            if reply.r#type() != x::ATOM_STRING || reply.format() != 8 {
                return Err(EvalError::BadResponse {
                    window: self.portal,
                    atom: self.property,
                    typ: reply.r#type(),
                    format: reply.format(),
                });
            }
            let bytes_after = reply.bytes_after();
            if bytes_after == 0 {
                break reply
                    .value::<u8>()
                    .split_first()
                    .map(|(status, data)| (*status == 1, data.to_vec()))
                    .ok_or(EvalError::NoResponse)?;
            }
            long_length += (bytes_after / 4) + 1;
        };
        Ok(if success { Ok(data) } else { Err(data) })
    }

    /// Loops waiting for a PropertyNotify event on the portal window.
    fn wait_for_property_notify(&mut self) -> Result<(), xcb::Error> {
        loop {
            let event = self.conn.wait_for_event()?;
            if let xcb::Event::X(x::Event::PropertyNotify(ev)) = event &&
                ev.window() == self.portal &&
                ev.atom() == self.property
            {
                return Ok(());
            }
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.conn.send_request(&x::DestroyWindow { window: self.portal });
    }
}
